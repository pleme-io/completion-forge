// OpenAPI spec → completion IR conversion.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use heck::ToKebabCase;

use crate::ir::{CommandGroup, CompletionFlag, CompletionOp, CompletionSpec, Glyph};
use crate::spec::{OpenApiSpec, PathItemExt};

// ── Grouping strategy ─────────────────────────────────────────────────────

/// How to group `OpenAPI` operations into subcommands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum GroupingStrategy {
    /// Try tag first, then path, then operation ID.
    #[default]
    Auto,
    /// Group by the first `OpenAPI` tag on each operation.
    ByTag,
    /// Group by the first non-parameter path segment.
    ByPath,
    /// Strip verb prefix from `operationId` → resource group.
    ByOperationId,
}

impl GroupingStrategy {
    /// Parse from string leniently, falling back to [`Auto`](Self::Auto) on
    /// unrecognised input. Prefer [`FromStr`] when you want an error instead.
    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}

impl FromStr for GroupingStrategy {
    type Err = crate::error::ParseEnumError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "tag" | "tags" | "by-tag" => Ok(Self::ByTag),
            "path" | "paths" | "by-path" => Ok(Self::ByPath),
            "operation" | "operation-id" | "by-operation-id" => Ok(Self::ByOperationId),
            _ => Err(crate::error::ParseEnumError(s.to_owned())),
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

/// Trait for converting an `OpenAPI` spec into a [`CompletionSpec`].
pub trait Converter: Send + Sync {
    /// Perform the conversion.
    fn convert(&self, spec: &OpenApiSpec) -> CompletionSpec;
}

/// Default converter that delegates to the free [`convert()`] function.
///
/// Use the builder methods to configure, then call
/// [`Converter::convert`] to produce a [`CompletionSpec`].
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

impl DefaultConverter {
    /// Create a new converter with the given command name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            icon: String::new(),
            aliases: Vec::new(),
            strategy: GroupingStrategy::default(),
        }
    }

    /// Set the prompt icon.
    #[must_use]
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    /// Set the command aliases.
    #[must_use]
    pub fn aliases(mut self, aliases: Vec<String>) -> Self {
        self.aliases = aliases;
        self
    }

    /// Set the grouping strategy.
    #[must_use]
    pub fn strategy(mut self, strategy: GroupingStrategy) -> Self {
        self.strategy = strategy;
        self
    }
}

impl Converter for DefaultConverter {
    fn convert(&self, spec: &OpenApiSpec) -> CompletionSpec {
        convert(spec, &self.name, &self.icon, &self.aliases, self.strategy)
    }
}

/// Convert an `OpenAPI` spec into a [`CompletionSpec`].
#[must_use]
pub fn convert(
    spec: &OpenApiSpec,
    name: &str,
    icon: &str,
    aliases: &[String],
    strategy: GroupingStrategy,
) -> CompletionSpec {
    let raw_ops: Vec<RawOp> = spec
        .paths
        .iter()
        .flat_map(|(path, item)| {
            let path_params = &item.parameters;
            item.operations()
                .into_iter()
                .map(move |(method, op)| RawOp::from_operation(path, method, op, path_params))
        })
        .collect();

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

    let mut groups_map: BTreeMap<String, Vec<RawOp>> = BTreeMap::new();
    for op in raw_ops {
        let key = op.group_key(effective);
        groups_map.entry(key).or_default().push(op);
    }

    let groups = groups_map
        .into_iter()
        .map(|(name, ops)| build_group(name, &ops))
        .collect();

    CompletionSpec {
        name: name.to_owned(),
        icon: icon.to_owned(),
        aliases: aliases.to_vec(),
        description: spec
            .info
            .description
            .clone()
            .unwrap_or_else(|| spec.info.title.clone()),
        groups,
    }
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

impl From<&RawParam> for CompletionFlag {
    fn from(p: &RawParam) -> Self {
        Self {
            name: p.name.clone(),
            description: p.description.clone(),
            required: p.required,
        }
    }
}

impl RawOp {
    fn group_key(&self, strategy: GroupingStrategy) -> String {
        match strategy {
            GroupingStrategy::ByTag | GroupingStrategy::Auto => self
                .tags
                .first()
                .map_or_else(|| path_group(&self.path), |t| t.to_kebab_case()),
            GroupingStrategy::ByPath => path_group(&self.path),
            GroupingStrategy::ByOperationId => operation_id_group(&self.operation_id),
        }
    }

    fn completion_name(&self) -> String {
        if !self.operation_id.is_empty() {
            return self.operation_id.to_kebab_case();
        }
        let path_part: String = self
            .path
            .split('/')
            .filter(|s| !s.is_empty() && !s.starts_with('{'))
            .collect::<Vec<_>>()
            .join("-");
        format!("{}-{path_part}", self.method.to_lowercase()).to_kebab_case()
    }

    fn from_operation(
        path: &str,
        method: &str,
        op: &crate::spec::Operation,
        path_params: &[crate::spec::Parameter],
    ) -> Self {
        let summary = first_non_empty(&[op.summary.as_deref(), op.description.as_deref()])
            .to_owned();
        Self {
            method: method.to_owned(),
            path: path.to_owned(),
            operation_id: op.operation_id.clone().unwrap_or_default(),
            summary,
            tags: op.tags.clone(),
            params: collect_params(path_params, &op.parameters),
            body_fields: collect_body_fields(op),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn build_group(group_name: String, ops: &[RawOp]) -> CommandGroup {
    let methods: Vec<&str> = ops.iter().map(|o| o.method.as_str()).collect();
    let glyph = Glyph::from_methods(&methods);

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

    let mut flags_map: BTreeMap<String, CompletionFlag> = BTreeMap::new();
    for op in ops {
        for p in op.params.iter().chain(&op.body_fields) {
            flags_map
                .entry(p.name.clone())
                .or_insert_with(|| CompletionFlag::from(p));
        }
    }

    let operations = ops
        .iter()
        .map(|o| CompletionOp {
            name: o.completion_name(),
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
}

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
    path_params
        .iter()
        .chain(op_params)
        .map(|p| RawParam {
            name: p.name.clone(),
            description: p.description.clone().unwrap_or_default(),
            required: p.required,
        })
        .collect()
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
    const PREFIXES: &[&str] = &[
        "list", "get", "create", "update", "delete", "remove", "add", "set", "put", "patch",
        "post", "find", "search", "fetch",
    ];
    for prefix in PREFIXES {
        if let Some(rest) = s
            .strip_prefix(prefix)
            .filter(|r| r.starts_with(|c: char| c.is_uppercase()))
        {
            return rest;
        }
    }
    s
}

fn format_group_description(group_name: &str) -> String {
    let spaced = group_name.replace('-', " ");
    let mut chars = spaced.chars();
    let capitalised: String = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    };
    format!("{capitalised} operations")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_op(method: &str, path: &str, operation_id: &str) -> RawOp {
        RawOp {
            method: method.into(),
            path: path.into(),
            operation_id: operation_id.into(),
            summary: String::new(),
            tags: vec![],
            params: vec![],
            body_fields: vec![],
        }
    }

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
        let result = convert(&spec, "petstore", "\u{2601}", &[], GroupingStrategy::ByTag);
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
            convert(&spec, "petstore", "\u{2601}", &[], GroupingStrategy::ByPath);
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
        );
        // listPets→Pets, createPet→Pet, getPet→Pet, deletePet→Pet, listStores→Stores
        assert!(result.groups.len() >= 2);
    }

    #[test]
    fn convert_auto_uses_tags() {
        let spec = petstore_spec();
        let result = convert(&spec, "test", "", &[], GroupingStrategy::Auto);
        // Auto should pick ByTag since tags exist
        assert_eq!(result.groups.len(), 2);
    }

    #[test]
    fn convert_with_aliases() {
        let spec = petstore_spec();
        let aliases = vec!["ps".into(), "pet".into()];
        let result =
            convert(&spec, "petstore", "\u{2601}", &aliases, GroupingStrategy::Auto);
        assert_eq!(result.aliases, vec!["ps", "pet"]);
    }

    #[test]
    fn flags_extracted_from_params() {
        let spec = petstore_spec();
        let result = convert(&spec, "test", "", &[], GroupingStrategy::ByTag);
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
        let result = convert(&spec, "test", "", &[], GroupingStrategy::ByTag);
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
        assert_eq!(
            raw_op("GET", "/pets", "listPets").completion_name(),
            "list-pets"
        );
        assert_eq!(
            raw_op("POST", "/pets", "createPet").completion_name(),
            "create-pet"
        );
    }

    #[test]
    fn op_name_fallback() {
        assert_eq!(
            raw_op("GET", "/pets/{petId}", "").completion_name(),
            "get-pets"
        );
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
        let result = convert(&spec, "empty", "", &[], GroupingStrategy::Auto);
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
        let key = op.group_key(GroupingStrategy::ByTag);
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
        let key = op.group_key(GroupingStrategy::ByPath);
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
        let result = converter.convert(&spec);
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

        let result = convert(&spec, "notags", "", &[], GroupingStrategy::Auto);
        // No tags, no operation IDs → Auto falls through to ByPath.
        // /users → "users", /orders → "orders"
        assert_eq!(result.groups.len(), 2);
        let group_names: Vec<&str> = result.groups.iter().map(|g| g.name.as_str()).collect();
        assert!(group_names.contains(&"users"));
        assert!(group_names.contains(&"orders"));
    }

    #[test]
    fn test_auto_strategy_picks_operation_id_when_no_tags() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: OpId Only API
  version: "1.0.0"
paths:
  /users:
    get:
      operationId: listUsers
      summary: List users
    post:
      operationId: createUser
      summary: Create user
  /orders:
    get:
      operationId: listOrders
      summary: List orders
"#,
        )
        .unwrap();

        let result = convert(&spec, "opid-api", "", &[], GroupingStrategy::Auto);
        let group_names: Vec<&str> = result.groups.iter().map(|g| g.name.as_str()).collect();
        assert!(group_names.contains(&"users"));
        assert!(group_names.contains(&"orders"));
        // "user" from createUser after verb stripping
        assert!(group_names.contains(&"user"));
    }

    #[test]
    fn test_operation_id_group_empty_id_returns_default() {
        assert_eq!(operation_id_group(""), "default");
    }

    #[test]
    fn test_operation_id_group_no_verb_prefix() {
        assert_eq!(operation_id_group("pets"), "pets");
    }

    #[test]
    fn test_operation_id_group_standard_verbs() {
        assert_eq!(operation_id_group("listPets"), "pets");
        assert_eq!(operation_id_group("getPet"), "pet");
        assert_eq!(operation_id_group("createUser"), "user");
        assert_eq!(operation_id_group("deletePet"), "pet");
        assert_eq!(operation_id_group("updateProfile"), "profile");
        assert_eq!(operation_id_group("removePet"), "pet");
        assert_eq!(operation_id_group("addItem"), "item");
        assert_eq!(operation_id_group("setPref"), "pref");
        assert_eq!(operation_id_group("findUsers"), "users");
        assert_eq!(operation_id_group("searchItems"), "items");
        assert_eq!(operation_id_group("fetchData"), "data");
    }

    #[test]
    fn test_group_key_by_operation_id() {
        let op = RawOp {
            method: "GET".into(),
            path: "/pets".into(),
            operation_id: "listPets".into(),
            summary: "List pets".into(),
            tags: vec![],
            params: vec![],
            body_fields: vec![],
        };
        let key = op.group_key(GroupingStrategy::ByOperationId);
        assert_eq!(key, "pets");
    }

    #[test]
    fn test_group_key_by_operation_id_empty() {
        let op = RawOp {
            method: "GET".into(),
            path: "/pets".into(),
            operation_id: String::new(),
            summary: "List pets".into(),
            tags: vec![],
            params: vec![],
            body_fields: vec![],
        };
        let key = op.group_key(GroupingStrategy::ByOperationId);
        assert_eq!(key, "default");
    }

    #[test]
    fn test_group_key_by_tag_falls_to_path_when_no_tags() {
        let op = RawOp {
            method: "GET".into(),
            path: "/users/{id}".into(),
            operation_id: "getUser".into(),
            summary: "Get user".into(),
            tags: vec![],
            params: vec![],
            body_fields: vec![],
        };
        let key = op.group_key(GroupingStrategy::ByTag);
        assert_eq!(key, "users");
    }

    #[test]
    fn test_collect_body_fields_wrong_media_type() {
        use crate::spec::{MediaType, Operation, RequestBody, Schema};
        use std::collections::BTreeMap;

        let mut content = BTreeMap::new();
        content.insert(
            "application/xml".to_owned(),
            MediaType {
                schema: Some(Schema {
                    schema_type: Some("object".into()),
                    properties: {
                        let mut p = BTreeMap::new();
                        p.insert(
                            "name".into(),
                            Schema {
                                schema_type: Some("string".into()),
                                ..Schema::default()
                            },
                        );
                        p
                    },
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
        assert!(fields.is_empty());
    }

    #[test]
    fn test_collect_body_fields_no_schema() {
        use crate::spec::{MediaType, Operation, RequestBody};
        use std::collections::BTreeMap;

        let mut content = BTreeMap::new();
        content.insert(
            "application/json".to_owned(),
            MediaType { schema: None },
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
        assert!(fields.is_empty());
    }

    #[test]
    fn test_collect_body_fields_empty_properties() {
        use crate::spec::{MediaType, Operation, RequestBody, Schema};
        use std::collections::BTreeMap;

        let mut content = BTreeMap::new();
        content.insert(
            "application/json".to_owned(),
            MediaType {
                schema: Some(Schema {
                    schema_type: Some("object".into()),
                    properties: BTreeMap::new(),
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
        assert!(fields.is_empty());
    }

    #[test]
    fn test_duplicate_flag_names_first_wins() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: Dup Flags
  version: "1.0.0"
paths:
  /items:
    parameters:
      - name: limit
        in: query
        required: true
        description: Path-level limit
    get:
      operationId: listItems
      summary: List items
      tags: [items]
      parameters:
        - name: limit
          in: query
          required: false
          description: Op-level limit
"#,
        )
        .unwrap();

        let result = convert(&spec, "test", "", &[], GroupingStrategy::ByTag);
        let items = result.groups.iter().find(|g| g.name == "items").unwrap();
        let limit_flags: Vec<&CompletionFlag> =
            items.flags.iter().filter(|f| f.name == "limit").collect();
        assert_eq!(limit_flags.len(), 1, "duplicate flags should be deduplicated");
        assert!(
            limit_flags[0].required,
            "first occurrence (path-level, required=true) should win"
        );
    }

    #[test]
    fn test_description_uses_info_title_when_no_description() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: My Title
  version: "1.0.0"
paths: {}
"#,
        )
        .unwrap();

        let result = convert(&spec, "test", "", &[], GroupingStrategy::Auto);
        assert_eq!(result.description, "My Title");
    }

    #[test]
    fn test_description_uses_info_description_when_present() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: My Title
  description: My Description
  version: "1.0.0"
paths: {}
"#,
        )
        .unwrap();

        let result = convert(&spec, "test", "", &[], GroupingStrategy::Auto);
        assert_eq!(result.description, "My Description");
    }

    #[test]
    fn test_single_op_group_uses_summary_not_format() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: Single Op
  version: "1.0.0"
paths:
  /health:
    get:
      operationId: healthCheck
      summary: Check service health
      tags: [monitoring]
"#,
        )
        .unwrap();

        let result = convert(&spec, "test", "", &[], GroupingStrategy::ByTag);
        let monitoring = result
            .groups
            .iter()
            .find(|g| g.name == "monitoring")
            .unwrap();
        assert_eq!(monitoring.description, "Check service health");
    }

    #[test]
    fn test_multi_op_group_uses_formatted_description() {
        let spec = petstore_spec();
        let result = convert(&spec, "test", "", &[], GroupingStrategy::ByTag);
        let pets = result.groups.iter().find(|g| g.name == "pets").unwrap();
        assert_eq!(pets.description, "Pets operations");
    }

    #[test]
    fn test_format_group_description_empty() {
        assert_eq!(format_group_description(""), " operations");
    }

    #[test]
    fn test_path_group_only_params() {
        assert_eq!(path_group("/{id}"), "default");
    }

    #[test]
    fn test_path_group_nested() {
        assert_eq!(path_group("/api/v1/users/{userId}/posts"), "api");
    }

    #[test]
    fn test_strip_verb_prefix_all_verbs() {
        assert_eq!(strip_verb_prefix("removePet"), "Pet");
        assert_eq!(strip_verb_prefix("addItem"), "Item");
        assert_eq!(strip_verb_prefix("setConfig"), "Config");
        assert_eq!(strip_verb_prefix("putResource"), "Resource");
        assert_eq!(strip_verb_prefix("patchField"), "Field");
        assert_eq!(strip_verb_prefix("postData"), "Data");
        assert_eq!(strip_verb_prefix("findUser"), "User");
        assert_eq!(strip_verb_prefix("searchResult"), "Result");
        assert_eq!(strip_verb_prefix("fetchItem"), "Item");
    }

    #[test]
    fn test_op_name_multi_segment_path() {
        assert_eq!(
            raw_op("GET", "/api/v1/users", "").completion_name(),
            "get-api-v1-users"
        );
    }

    #[test]
    fn test_op_name_path_with_only_params() {
        assert_eq!(raw_op("DELETE", "/{id}", "").completion_name(), "delete");
    }

    #[test]
    fn test_grouping_strategy_from_str_aliases() {
        assert!(matches!(
            GroupingStrategy::from_str_loose("tags"),
            GroupingStrategy::ByTag
        ));
        assert!(matches!(
            GroupingStrategy::from_str_loose("by-tag"),
            GroupingStrategy::ByTag
        ));
        assert!(matches!(
            GroupingStrategy::from_str_loose("paths"),
            GroupingStrategy::ByPath
        ));
        assert!(matches!(
            GroupingStrategy::from_str_loose("operation"),
            GroupingStrategy::ByOperationId
        ));
        assert!(matches!(
            GroupingStrategy::from_str_loose("by-operation-id"),
            GroupingStrategy::ByOperationId
        ));
    }

    #[test]
    fn test_grouping_strategy_from_str_case_insensitive() {
        assert!(matches!(
            GroupingStrategy::from_str_loose("TAG"),
            GroupingStrategy::ByTag
        ));
        assert!(matches!(
            GroupingStrategy::from_str_loose("Path"),
            GroupingStrategy::ByPath
        ));
        assert!(matches!(
            GroupingStrategy::from_str_loose("OPERATION-ID"),
            GroupingStrategy::ByOperationId
        ));
    }

    #[test]
    fn test_collect_params_empty() {
        let result = collect_params(&[], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_params_description_fallback() {
        use crate::spec::Parameter;

        let params = vec![Parameter {
            name: "id".into(),
            location: "path".into(),
            required: true,
            description: None,
            schema: None,
            ref_path: None,
        }];

        let result = collect_params(&params, &[]);
        assert_eq!(result[0].description, "");
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

        let result = convert(&spec, "bare", "", &[], GroupingStrategy::Auto);
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

    #[test]
    fn grouping_strategy_default_is_auto() {
        let strategy: GroupingStrategy = GroupingStrategy::default();
        assert!(matches!(strategy, GroupingStrategy::Auto));
    }

    #[test]
    fn build_group_single_op_uses_summary() {
        let ops = vec![RawOp {
            method: "GET".into(),
            path: "/health".into(),
            operation_id: "healthCheck".into(),
            summary: "Health check endpoint".into(),
            tags: vec![],
            params: vec![],
            body_fields: vec![],
        }];
        let group = build_group("health".into(), &ops);
        assert_eq!(group.name, "health");
        assert_eq!(group.description, "Health check endpoint");
        assert_eq!(group.glyph, Glyph::View);
        assert_eq!(group.operations.len(), 1);
        assert_eq!(group.operations[0].name, "health-check");
    }

    #[test]
    fn build_group_multiple_ops_uses_formatted_desc() {
        let ops = vec![
            RawOp {
                method: "GET".into(),
                path: "/items".into(),
                operation_id: "listItems".into(),
                summary: "List items".into(),
                tags: vec![],
                params: vec![],
                body_fields: vec![],
            },
            RawOp {
                method: "POST".into(),
                path: "/items".into(),
                operation_id: "createItem".into(),
                summary: "Create item".into(),
                tags: vec![],
                params: vec![],
                body_fields: vec![],
            },
        ];
        let group = build_group("items".into(), &ops);
        assert_eq!(group.description, "Items operations");
        assert_eq!(group.glyph, Glyph::Manage);
        assert_eq!(group.operations.len(), 2);
    }

    #[test]
    fn build_group_deduplicates_flags() {
        let ops = vec![
            RawOp {
                method: "GET".into(),
                path: "/items".into(),
                operation_id: "listItems".into(),
                summary: "List".into(),
                tags: vec![],
                params: vec![RawParam {
                    name: "limit".into(),
                    description: "First limit".into(),
                    required: true,
                }],
                body_fields: vec![],
            },
            RawOp {
                method: "POST".into(),
                path: "/items".into(),
                operation_id: "createItem".into(),
                summary: "Create".into(),
                tags: vec![],
                params: vec![RawParam {
                    name: "limit".into(),
                    description: "Second limit".into(),
                    required: false,
                }],
                body_fields: vec![],
            },
        ];
        let group = build_group("items".into(), &ops);
        let limit_flags: Vec<_> = group.flags.iter().filter(|f| f.name == "limit").collect();
        assert_eq!(limit_flags.len(), 1, "duplicate flags should be deduplicated");
        assert!(limit_flags[0].required, "first occurrence should win");
        assert_eq!(limit_flags[0].description, "First limit");
    }

    #[test]
    fn build_group_collects_body_fields_as_flags() {
        let ops = vec![RawOp {
            method: "POST".into(),
            path: "/items".into(),
            operation_id: "createItem".into(),
            summary: "Create".into(),
            tags: vec![],
            params: vec![],
            body_fields: vec![
                RawParam {
                    name: "title".into(),
                    description: "Item title".into(),
                    required: false,
                },
                RawParam {
                    name: "count".into(),
                    description: "Item count".into(),
                    required: false,
                },
            ],
        }];
        let group = build_group("items".into(), &ops);
        assert_eq!(group.flags.len(), 2);
        let flag_names: Vec<&str> = group.flags.iter().map(|f| f.name.as_str()).collect();
        assert!(flag_names.contains(&"title"));
        assert!(flag_names.contains(&"count"));
    }

    #[test]
    fn build_group_empty_ops() {
        let group = build_group("empty".into(), &[]);
        assert_eq!(group.name, "empty");
        assert_eq!(group.description, "");
        assert_eq!(group.glyph, Glyph::Manage);
        assert!(group.operations.is_empty());
        assert!(group.flags.is_empty());
    }

    #[test]
    fn convert_preserves_operation_methods() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: Methods API
  version: "1.0.0"
paths:
  /items:
    get:
      operationId: listItems
      summary: List items
      tags: [items]
    post:
      operationId: createItem
      summary: Create item
      tags: [items]
    put:
      operationId: updateItem
      summary: Update item
      tags: [items]
"#,
        )
        .unwrap();
        let result = convert(&spec, "test", "", &[], GroupingStrategy::ByTag);
        let items = result.groups.iter().find(|g| g.name == "items").unwrap();
        let methods: Vec<&str> = items.operations.iter().map(|o| o.method.as_str()).collect();
        assert!(methods.contains(&"GET"));
        assert!(methods.contains(&"POST"));
        assert!(methods.contains(&"PUT"));
    }

    #[test]
    fn test_default_converter_builder() {
        let spec = petstore_spec();
        let converter = DefaultConverter::new("petstore")
            .icon("\u{2601}")
            .aliases(vec!["ps".into()])
            .strategy(GroupingStrategy::ByTag);
        let result = converter.convert(&spec);
        assert_eq!(result.name, "petstore");
        assert_eq!(result.icon, "\u{2601}");
        assert_eq!(result.aliases, vec!["ps"]);
        assert_eq!(result.groups.len(), 2);
    }

    #[test]
    fn test_default_converter_builder_defaults() {
        let spec = petstore_spec();
        let converter = DefaultConverter::new("petstore");
        assert!(converter.icon.is_empty());
        assert!(converter.aliases.is_empty());
        assert_eq!(converter.strategy, GroupingStrategy::Auto);
        let result = converter.convert(&spec);
        assert_eq!(result.name, "petstore");
    }

    struct MockConverter {
        spec: CompletionSpec,
    }

    impl Converter for MockConverter {
        fn convert(&self, _spec: &OpenApiSpec) -> CompletionSpec {
            self.spec.clone()
        }
    }

    #[test]
    fn mock_converter_returns_canned_spec() {
        let spec = petstore_spec();
        let canned = CompletionSpec {
            name: "mock-tool".into(),
            icon: "M".into(),
            ..CompletionSpec::default()
        };
        let mock = MockConverter {
            spec: canned.clone(),
        };
        let result = mock.convert(&spec);
        assert_eq!(result.name, "mock-tool");
        assert_eq!(result.icon, "M");
        assert!(result.groups.is_empty());
    }

    #[test]
    fn converter_trait_object_dispatches() {
        let spec = petstore_spec();
        let converters: Vec<Box<dyn Converter>> = vec![
            Box::new(DefaultConverter::new("petstore").strategy(GroupingStrategy::ByTag)),
            Box::new(MockConverter {
                spec: CompletionSpec {
                    name: "mock".into(),
                    ..CompletionSpec::default()
                },
            }),
        ];

        let results: Vec<String> = converters
            .iter()
            .map(|c| c.convert(&spec).name)
            .collect();

        assert_eq!(results, vec!["petstore", "mock"]);
    }

    #[test]
    fn grouping_strategy_from_str_valid() {
        assert_eq!(
            "auto".parse::<GroupingStrategy>().unwrap(),
            GroupingStrategy::Auto
        );
        assert_eq!(
            "tag".parse::<GroupingStrategy>().unwrap(),
            GroupingStrategy::ByTag
        );
        assert_eq!(
            "path".parse::<GroupingStrategy>().unwrap(),
            GroupingStrategy::ByPath
        );
        assert_eq!(
            "operation-id".parse::<GroupingStrategy>().unwrap(),
            GroupingStrategy::ByOperationId
        );
    }

    #[test]
    fn grouping_strategy_from_str_invalid() {
        assert!("nope".parse::<GroupingStrategy>().is_err());
    }

    #[test]
    fn grouping_strategy_display_from_str_roundtrip() {
        let variants = [
            GroupingStrategy::Auto,
            GroupingStrategy::ByTag,
            GroupingStrategy::ByPath,
            GroupingStrategy::ByOperationId,
        ];
        for v in &variants {
            let s = v.to_string();
            let parsed: GroupingStrategy = s.parse().unwrap_or_else(|_| {
                panic!("failed to parse GroupingStrategy from Display output: {s}")
            });
            assert_eq!(*v, parsed, "round-trip failed for {s}");
        }
    }
}
