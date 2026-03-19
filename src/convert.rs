// OpenAPI spec → completion IR conversion.

use std::collections::BTreeMap;
use std::fmt;

use anyhow::Result;
use heck::ToKebabCase;

use crate::ir::{CommandGroup, CompletionFlag, CompletionOp, CompletionSpec, Glyph};
use crate::spec::{OpenApiSpec, PathItemExt};

// ── Grouping strategy ─────────────────────────────────────────────────────

/// How to group OpenAPI operations into subcommands.
#[derive(Debug, Clone, Copy, Default)]
pub enum GroupingStrategy {
    /// Try tag first, then path, then operation ID.
    #[default]
    Auto,
    /// Group by the first OpenAPI tag on each operation.
    ByTag,
    /// Group by the first non-parameter path segment.
    ByPath,
    /// Strip verb prefix from `operationId` → resource group.
    ByOperationId,
}

impl GroupingStrategy {
    /// Parse from string (for CLI).
    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "tag" | "tags" | "by-tag" => Self::ByTag,
            "path" | "paths" | "by-path" => Self::ByPath,
            "operation" | "operation-id" | "by-operation-id" => Self::ByOperationId,
            _ => Self::Auto,
        }
    }
}

impl fmt::Display for GroupingStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::ByTag => write!(f, "by-tag"),
            Self::ByPath => write!(f, "by-path"),
            Self::ByOperationId => write!(f, "by-operation-id"),
        }
    }
}

// ── Conversion ────────────────────────────────────────────────────────────

// ── Converter trait ───────────────────────────────────────────────────

/// Trait for converting an OpenAPI spec into a `CompletionSpec`.
pub trait Converter: Send + Sync {
    /// Perform the conversion.
    ///
    /// # Errors
    /// Returns an error if the spec cannot be converted.
    fn convert(&self, spec: &OpenApiSpec) -> Result<CompletionSpec>;
}

/// Default converter that delegates to the free `convert()` function.
pub struct DefaultConverter {
    /// CLI command name.
    pub name: String,
    /// Prompt icon (Unicode glyph).
    pub icon: String,
    /// Command aliases.
    pub aliases: Vec<String>,
    /// Grouping strategy.
    pub strategy: GroupingStrategy,
}

impl Converter for DefaultConverter {
    fn convert(&self, spec: &OpenApiSpec) -> Result<CompletionSpec> {
        convert(spec, &self.name, &self.icon, &self.aliases, self.strategy)
    }
}

/// Convert an OpenAPI spec into a `CompletionSpec`.
///
/// # Errors
/// Returns an error if the spec cannot be converted.
pub fn convert(
    spec: &OpenApiSpec,
    name: &str,
    icon: &str,
    aliases: &[String],
    strategy: GroupingStrategy,
) -> Result<CompletionSpec> {
    // Collect all operations with their metadata.
    let mut raw_ops: Vec<RawOp> = Vec::new();
    for (path, item) in &spec.paths {
        let path_params = &item.parameters;
        for (method, op) in item.operations() {
            let summary = first_non_empty(&[
                op.summary.as_deref(),
                op.description.as_deref(),
            ])
            .to_owned();
            raw_ops.push(RawOp {
                method: method.to_owned(),
                path: path.clone(),
                operation_id: op.operation_id.clone().unwrap_or_default(),
                summary,
                tags: op.tags.clone(),
                params: collect_params(path_params, &op.parameters),
                body_fields: collect_body_fields(op),
            });
        }
    }

    // Determine effective strategy.
    let effective = match strategy {
        GroupingStrategy::Auto => {
            if raw_ops.iter().any(|o| !o.tags.is_empty()) {
                GroupingStrategy::ByTag
            } else if raw_ops.iter().any(|o| !o.operation_id.is_empty()) {
                GroupingStrategy::ByOperationId
            } else {
                GroupingStrategy::ByPath
            }
        }
        other => other,
    };

    // Group operations.
    let mut groups_map: BTreeMap<String, Vec<RawOp>> = BTreeMap::new();
    for op in raw_ops {
        let key = group_key(&op, effective);
        groups_map.entry(key).or_default().push(op);
    }

    // Convert groups to IR.
    let groups = groups_map
        .into_iter()
        .map(|(group_name, ops)| {
            let methods: Vec<&str> = ops.iter().map(|o| o.method.as_str()).collect();
            let glyph = Glyph::from_methods(&methods);

            // Pick the best description from the operations.
            let description = ops
                .first()
                .map(|o| {
                    if ops.len() == 1 {
                        o.summary.clone()
                    } else {
                        format_group_description(&group_name)
                    }
                })
                .unwrap_or_default();

            // Collect unique flags from all operations in group.
            let mut flags_map: BTreeMap<String, CompletionFlag> = BTreeMap::new();
            for op in &ops {
                for p in &op.params {
                    flags_map
                        .entry(p.name.clone())
                        .or_insert_with(|| CompletionFlag {
                            name: p.name.clone(),
                            description: p.description.clone(),
                            required: p.required,
                        });
                }
                for f in &op.body_fields {
                    flags_map
                        .entry(f.name.clone())
                        .or_insert_with(|| CompletionFlag {
                            name: f.name.clone(),
                            description: f.description.clone(),
                            required: f.required,
                        });
                }
            }

            let operations = ops
                .iter()
                .map(|o| CompletionOp {
                    name: op_name(&o.operation_id, &o.path, &o.method),
                    description: o.summary.clone(),
                    method: o.method.clone(),
                })
                .collect();

            CommandGroup {
                name: group_name,
                description,
                glyph,
                operations,
                flags: flags_map.into_values().collect(),
            }
        })
        .collect();

    Ok(CompletionSpec {
        name: name.to_owned(),
        icon: icon.to_owned(),
        aliases: aliases.to_vec(),
        description: spec
            .info
            .description
            .clone()
            .unwrap_or_else(|| spec.info.title.clone()),
        groups,
    })
}

// ── Internal types ────────────────────────────────────────────────────────

struct RawOp {
    method: String,
    path: String,
    operation_id: String,
    summary: String,
    tags: Vec<String>,
    params: Vec<RawParam>,
    body_fields: Vec<RawParam>,
}

struct RawParam {
    name: String,
    description: String,
    required: bool,
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Return the first non-empty `&str` from a slice of options, or `""`.
fn first_non_empty<'a>(values: &[Option<&'a str>]) -> &'a str {
    values
        .iter()
        .copied()
        .find_map(|v| v.filter(|s| !s.is_empty()))
        .unwrap_or_default()
}

fn collect_params(
    path_params: &[crate::spec::Parameter],
    op_params: &[crate::spec::Parameter],
) -> Vec<RawParam> {
    let mut result = Vec::new();
    for p in path_params.iter().chain(op_params) {
        result.push(RawParam {
            name: p.name.clone(),
            description: p.description.clone().unwrap_or_default(),
            required: p.required,
        });
    }
    result
}

fn collect_body_fields(op: &crate::spec::Operation) -> Vec<RawParam> {
    let Some(ref body) = op.request_body else {
        return Vec::new();
    };
    let Some(media) = body.content.get("application/json") else {
        return Vec::new();
    };
    let Some(ref schema) = media.schema else {
        return Vec::new();
    };

    schema
        .properties
        .iter()
        .map(|(name, prop)| RawParam {
            name: name.clone(),
            description: prop.description.clone().unwrap_or_default(),
            required: false,
        })
        .collect()
}

fn group_key(op: &RawOp, strategy: GroupingStrategy) -> String {
    match strategy {
        GroupingStrategy::ByTag | GroupingStrategy::Auto => op
            .tags
            .first()
            .map_or_else(|| path_group(&op.path), |t| t.to_kebab_case()),
        GroupingStrategy::ByPath => path_group(&op.path),
        GroupingStrategy::ByOperationId => operation_id_group(&op.operation_id),
    }
}

/// Extract group name from path: `/pets/{petId}` → `"pets"`.
fn path_group(path: &str) -> String {
    path.split('/')
        .find(|s| !s.is_empty() && !s.starts_with('{'))
        .unwrap_or("default")
        .to_kebab_case()
}

/// Extract group from operation ID: `listPets` → `"pets"`, `createUser` → `"user"`.
fn operation_id_group(op_id: &str) -> String {
    if op_id.is_empty() {
        return "default".to_owned();
    }
    // Strip common verb prefixes.
    let stripped = strip_verb_prefix(op_id);
    stripped.to_kebab_case()
}

fn strip_verb_prefix(s: &str) -> &str {
    let prefixes = [
        "list", "get", "create", "update", "delete", "remove", "add", "set", "put", "patch",
        "post", "find", "search", "fetch",
    ];
    for prefix in &prefixes {
        if let Some(rest) = s
            .strip_prefix(prefix)
            .filter(|r| r.starts_with(|c: char| c.is_uppercase()))
        {
            return rest;
        }
    }
    s
}

fn op_name(operation_id: &str, path: &str, method: &str) -> String {
    if !operation_id.is_empty() {
        return operation_id.to_kebab_case();
    }
    // Fallback: method + path segments.
    let path_part: String = path
        .split('/')
        .filter(|s| !s.is_empty() && !s.starts_with('{'))
        .collect::<Vec<_>>()
        .join("-");
    format!("{}-{path_part}", method.to_lowercase()).to_kebab_case()
}

fn format_group_description(group_name: &str) -> String {
    let mut chars = group_name.replace('-', " ").chars().collect::<Vec<_>>();
    if let Some(c) = chars.first_mut() {
        *c = c.to_uppercase().next().unwrap_or(*c);
    }
    let desc: String = chars.into_iter().collect();
    format!("{desc} operations")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn petstore_spec() -> OpenApiSpec {
        serde_yaml_ng::from_str(
            r#"
info:
  title: Pet Store
  description: A sample pet store
  version: "1.0.0"
paths:
  /pets:
    get:
      operationId: listPets
      summary: List all pets
      tags: [pets]
      parameters:
        - name: limit
          in: query
          required: false
          description: Maximum number of results
    post:
      operationId: createPet
      summary: Create a pet
      tags: [pets]
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                name:
                  type: string
                  description: Pet name
  /pets/{petId}:
    parameters:
      - name: petId
        in: path
        required: true
        description: Pet identifier
    get:
      operationId: getPet
      summary: Get a pet
      tags: [pets]
    delete:
      operationId: deletePet
      summary: Delete a pet
      tags: [pets]
  /stores:
    get:
      operationId: listStores
      summary: List stores
      tags: [stores]
"#,
        )
        .unwrap()
    }

    #[test]
    fn convert_by_tag() {
        let spec = petstore_spec();
        let result = convert(&spec, "petstore", "\u{2601}", &[], GroupingStrategy::ByTag).unwrap();
        assert_eq!(result.name, "petstore");
        assert_eq!(result.icon, "\u{2601}");
        assert_eq!(result.groups.len(), 2);

        let pets = result.groups.iter().find(|g| g.name == "pets").unwrap();
        assert_eq!(pets.operations.len(), 4);
        assert_eq!(pets.glyph, Glyph::Manage); // mixed methods

        let stores = result.groups.iter().find(|g| g.name == "stores").unwrap();
        assert_eq!(stores.operations.len(), 1);
        assert_eq!(stores.glyph, Glyph::View); // all GET
    }

    #[test]
    fn convert_by_path() {
        let spec = petstore_spec();
        let result =
            convert(&spec, "petstore", "\u{2601}", &[], GroupingStrategy::ByPath).unwrap();
        // /pets and /pets/{petId} both group to "pets", /stores to "stores"
        assert_eq!(result.groups.len(), 2);
    }

    #[test]
    fn convert_by_operation_id() {
        let spec = petstore_spec();
        let result = convert(
            &spec,
            "petstore",
            "\u{2601}",
            &[],
            GroupingStrategy::ByOperationId,
        )
        .unwrap();
        // listPets→Pets, createPet→Pet, getPet→Pet, deletePet→Pet, listStores→Stores
        assert!(result.groups.len() >= 2);
    }

    #[test]
    fn convert_auto_uses_tags() {
        let spec = petstore_spec();
        let result = convert(&spec, "test", "", &[], GroupingStrategy::Auto).unwrap();
        // Auto should pick ByTag since tags exist
        assert_eq!(result.groups.len(), 2);
    }

    #[test]
    fn convert_with_aliases() {
        let spec = petstore_spec();
        let aliases = vec!["ps".into(), "pet".into()];
        let result =
            convert(&spec, "petstore", "\u{2601}", &aliases, GroupingStrategy::Auto).unwrap();
        assert_eq!(result.aliases, vec!["ps", "pet"]);
    }

    #[test]
    fn flags_extracted_from_params() {
        let spec = petstore_spec();
        let result = convert(&spec, "test", "", &[], GroupingStrategy::ByTag).unwrap();
        let pets = result.groups.iter().find(|g| g.name == "pets").unwrap();
        // Should have "limit" from query param and "petId" from path param and "name" from body
        let flag_names: Vec<&str> = pets.flags.iter().map(|f| f.name.as_str()).collect();
        assert!(flag_names.contains(&"limit"));
        assert!(flag_names.contains(&"petId"));
        assert!(flag_names.contains(&"name"));
    }

    #[test]
    fn glyph_auto_assignment() {
        let spec = petstore_spec();
        let result = convert(&spec, "test", "", &[], GroupingStrategy::ByTag).unwrap();
        let stores = result.groups.iter().find(|g| g.name == "stores").unwrap();
        assert_eq!(stores.glyph, Glyph::View); // only GET operations
    }

    #[test]
    fn path_group_extraction() {
        assert_eq!(path_group("/pets/{petId}"), "pets");
        assert_eq!(path_group("/api/v1/users"), "api");
        assert_eq!(path_group("/"), "default");
    }

    #[test]
    fn strip_verb_prefix_works() {
        assert_eq!(strip_verb_prefix("listPets"), "Pets");
        assert_eq!(strip_verb_prefix("createUser"), "User");
        assert_eq!(strip_verb_prefix("getPet"), "Pet");
        assert_eq!(strip_verb_prefix("deletePet"), "Pet");
        assert_eq!(strip_verb_prefix("unknown"), "unknown");
        // Should not strip if next char is lowercase.
        assert_eq!(strip_verb_prefix("listen"), "listen");
    }

    #[test]
    fn op_name_from_operation_id() {
        assert_eq!(op_name("listPets", "/pets", "GET"), "list-pets");
        assert_eq!(op_name("createPet", "/pets", "POST"), "create-pet");
    }

    #[test]
    fn op_name_fallback() {
        assert_eq!(op_name("", "/pets/{petId}", "GET"), "get-pets");
    }

    #[test]
    fn no_paths_produces_empty_groups() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: Empty
  version: "1.0.0"
paths: {}
"#,
        )
        .unwrap();
        let result = convert(&spec, "empty", "", &[], GroupingStrategy::Auto).unwrap();
        assert!(result.groups.is_empty());
    }

    #[test]
    fn grouping_strategy_from_str() {
        assert!(matches!(
            GroupingStrategy::from_str_loose("tag"),
            GroupingStrategy::ByTag
        ));
        assert!(matches!(
            GroupingStrategy::from_str_loose("by-path"),
            GroupingStrategy::ByPath
        ));
        assert!(matches!(
            GroupingStrategy::from_str_loose("operation-id"),
            GroupingStrategy::ByOperationId
        ));
        assert!(matches!(
            GroupingStrategy::from_str_loose("anything"),
            GroupingStrategy::Auto
        ));
    }

    #[test]
    fn grouping_strategy_display() {
        assert_eq!(GroupingStrategy::Auto.to_string(), "auto");
        assert_eq!(GroupingStrategy::ByTag.to_string(), "by-tag");
        assert_eq!(GroupingStrategy::ByPath.to_string(), "by-path");
        assert_eq!(GroupingStrategy::ByOperationId.to_string(), "by-operation-id");
    }

    #[test]
    fn test_collect_params_merges_path_and_op() {
        use crate::spec::Parameter;

        let path_params = vec![Parameter {
            name: "petId".into(),
            location: "path".into(),
            required: true,
            description: Some("Pet identifier".into()),
            schema: None,
            ref_path: None,
        }];
        let op_params = vec![Parameter {
            name: "limit".into(),
            location: "query".into(),
            required: false,
            description: Some("Max results".into()),
            schema: None,
            ref_path: None,
        }];

        let result = collect_params(&path_params, &op_params);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "petId");
        assert!(result[0].required);
        assert_eq!(result[1].name, "limit");
        assert!(!result[1].required);
    }

    #[test]
    fn test_collect_body_fields_from_json() {
        use crate::spec::{MediaType, Operation, RequestBody, Schema};
        use std::collections::BTreeMap;

        let mut properties = BTreeMap::new();
        properties.insert(
            "name".to_owned(),
            Schema {
                schema_type: Some("string".into()),
                description: Some("Pet name".into()),
                ..Schema::default()
            },
        );
        properties.insert(
            "age".to_owned(),
            Schema {
                schema_type: Some("integer".into()),
                ..Schema::default()
            },
        );

        let mut content = BTreeMap::new();
        content.insert(
            "application/json".to_owned(),
            MediaType {
                schema: Some(Schema {
                    schema_type: Some("object".into()),
                    properties,
                    ..Schema::default()
                }),
            },
        );

        let op = Operation {
            operation_id: None,
            summary: None,
            description: None,
            parameters: vec![],
            request_body: Some(RequestBody {
                required: false,
                content,
                description: None,
                ref_path: None,
            }),
            responses: BTreeMap::new(),
            security: vec![],
            tags: vec![],
        };

        let fields = collect_body_fields(&op);
        assert_eq!(fields.len(), 2);
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"age"));
    }

    #[test]
    fn test_collect_body_fields_no_body() {
        use crate::spec::Operation;
        use std::collections::BTreeMap;

        let op = Operation {
            operation_id: None,
            summary: None,
            description: None,
            parameters: vec![],
            request_body: None,
            responses: BTreeMap::new(),
            security: vec![],
            tags: vec![],
        };

        let fields = collect_body_fields(&op);
        assert!(fields.is_empty());
    }

    #[test]
    fn test_format_group_description() {
        assert_eq!(format_group_description("pets"), "Pets operations");
        assert_eq!(
            format_group_description("user-accounts"),
            "User accounts operations"
        );
    }

    #[test]
    fn test_group_key_by_tag() {
        let op = RawOp {
            method: "GET".into(),
            path: "/pets".into(),
            operation_id: "listPets".into(),
            summary: "List pets".into(),
            tags: vec!["Animals".into(), "Other".into()],
            params: vec![],
            body_fields: vec![],
        };
        let key = group_key(&op, GroupingStrategy::ByTag);
        assert_eq!(key, "animals");
    }

    #[test]
    fn test_group_key_by_path() {
        let op = RawOp {
            method: "GET".into(),
            path: "/pets/{petId}".into(),
            operation_id: "getPet".into(),
            summary: "Get pet".into(),
            tags: vec!["Animals".into()],
            params: vec![],
            body_fields: vec![],
        };
        let key = group_key(&op, GroupingStrategy::ByPath);
        assert_eq!(key, "pets");
    }

    #[test]
    fn test_first_non_empty() {
        assert_eq!(first_non_empty(&[Some("hello"), Some("world")]), "hello");
        assert_eq!(first_non_empty(&[None, Some("world")]), "world");
        assert_eq!(first_non_empty(&[Some(""), Some("world")]), "world");
        assert_eq!(first_non_empty(&[None, None]), "");
        assert_eq!(first_non_empty(&[]), "");
    }

    #[test]
    fn test_default_converter() {
        let spec = petstore_spec();
        let converter = DefaultConverter {
            name: "petstore".into(),
            icon: "\u{2601}".into(),
            aliases: vec!["ps".into()],
            strategy: GroupingStrategy::Auto,
        };
        let result = converter.convert(&spec).unwrap();
        assert_eq!(result.name, "petstore");
        assert_eq!(result.aliases, vec!["ps"]);
        assert_eq!(result.groups.len(), 2);
    }

    #[test]
    fn test_convert_operation_without_tags_falls_to_path() {
        // Spec with operations that have no tags — Auto should fall to ByPath
        // (or ByOperationId if operation IDs exist).
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: No Tags API
  version: "1.0.0"
paths:
  /users:
    get:
      summary: List users
    post:
      summary: Create user
  /orders:
    get:
      summary: List orders
"#,
        )
        .unwrap();

        let result = convert(&spec, "notags", "", &[], GroupingStrategy::Auto).unwrap();
        // No tags, no operation IDs → Auto falls through to ByPath.
        // /users → "users", /orders → "orders"
        assert_eq!(result.groups.len(), 2);
        let group_names: Vec<&str> = result.groups.iter().map(|g| g.name.as_str()).collect();
        assert!(group_names.contains(&"users"));
        assert!(group_names.contains(&"orders"));
    }

    #[test]
    fn test_convert_operation_without_id_or_tags_uses_path() {
        // No operation IDs, no tags — everything must fall back to path-based naming.
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: Bare API
  version: "1.0.0"
paths:
  /items/{itemId}:
    get:
      summary: Get item
    delete:
      summary: Delete item
"#,
        )
        .unwrap();

        let result = convert(&spec, "bare", "", &[], GroupingStrategy::Auto).unwrap();
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].name, "items");
        // Operations should be named from method + path since no operation ID.
        assert_eq!(result.groups[0].operations.len(), 2);
        let op_names: Vec<&str> = result.groups[0]
            .operations
            .iter()
            .map(|o| o.name.as_str())
            .collect();
        assert!(op_names.contains(&"get-items"));
        assert!(op_names.contains(&"delete-items"));
    }
}
