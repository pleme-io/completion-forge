// Minimal OpenAPI 3.0 serde types — only what completion-forge needs.

use std::collections::BTreeMap;

use serde::Deserialize;

// ── Root ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct OpenApiSpec {
    pub info: Info,
    #[serde(default)]
    pub paths: BTreeMap<String, PathItem>,
}

// ── Info ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Info {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub version: String,
}

// ── Paths & Operations ────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct PathItem {
    #[serde(default)]
    pub get: Option<Operation>,
    #[serde(default)]
    pub post: Option<Operation>,
    #[serde(default)]
    pub put: Option<Operation>,
    #[serde(default)]
    pub delete: Option<Operation>,
    #[serde(default)]
    pub patch: Option<Operation>,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    #[serde(default)]
    pub request_body: Option<RequestBody>,
    #[serde(default)]
    pub tags: Vec<String>,
}

// ── Parameters ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub schema: Option<Schema>,
}

// ── Request Body ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct RequestBody {
    #[serde(default)]
    pub content: BTreeMap<String, MediaType>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaType {
    #[serde(default)]
    pub schema: Option<Schema>,
}

// ── Schema (minimal) ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    #[serde(rename = "type", default)]
    pub schema_type: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, Schema>,
    #[serde(default)]
    pub description: Option<String>,
}

impl PathItem {
    /// Iterate over all (method_name, operation) pairs in this path.
    pub fn operations(&self) -> Vec<(&str, &Operation)> {
        let mut ops = Vec::new();
        if let Some(ref op) = self.get {
            ops.push(("GET", op));
        }
        if let Some(ref op) = self.post {
            ops.push(("POST", op));
        }
        if let Some(ref op) = self.put {
            ops.push(("PUT", op));
        }
        if let Some(ref op) = self.delete {
            ops.push(("DELETE", op));
        }
        if let Some(ref op) = self.patch {
            ops.push(("PATCH", op));
        }
        ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_SPEC: &str = r#"
info:
  title: Test API
  version: "1.0.0"
paths: {}
"#;

    const PETSTORE_SPEC: &str = r#"
info:
  title: Pet Store
  description: A sample pet store API
  version: "2.0.0"
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
          schema:
            type: integer
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
                  description: The pet name
  /pets/{petId}:
    parameters:
      - name: petId
        in: path
        required: true
    get:
      operationId: getPet
      summary: Get a pet by ID
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
"#;

    #[test]
    fn parse_minimal() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(MINIMAL_SPEC).unwrap();
        assert_eq!(spec.info.title, "Test API");
        assert!(spec.paths.is_empty());
    }

    #[test]
    fn parse_petstore() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(PETSTORE_SPEC).unwrap();
        assert_eq!(spec.info.title, "Pet Store");
        assert_eq!(spec.paths.len(), 3);
    }

    #[test]
    fn path_item_operations() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(PETSTORE_SPEC).unwrap();
        let pets = &spec.paths["/pets"];
        let ops = pets.operations();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].0, "GET");
        assert_eq!(ops[1].0, "POST");
    }

    #[test]
    fn parse_parameters() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(PETSTORE_SPEC).unwrap();
        let get_op = spec.paths["/pets"].get.as_ref().unwrap();
        assert_eq!(get_op.parameters.len(), 1);
        assert_eq!(get_op.parameters[0].name, "limit");
        assert_eq!(get_op.parameters[0].location, "query");
    }

    #[test]
    fn parse_tags() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(PETSTORE_SPEC).unwrap();
        let get_op = spec.paths["/pets"].get.as_ref().unwrap();
        assert_eq!(get_op.tags, vec!["pets"]);
    }

    #[test]
    fn parse_request_body() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(PETSTORE_SPEC).unwrap();
        let post_op = spec.paths["/pets"].post.as_ref().unwrap();
        let body = post_op.request_body.as_ref().unwrap();
        assert!(body.content.contains_key("application/json"));
    }

    #[test]
    fn parse_json_format() {
        let json = r#"{
            "info": { "title": "JSON API", "version": "0.1.0" },
            "paths": {}
        }"#;
        let spec: OpenApiSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.info.title, "JSON API");
    }
}
