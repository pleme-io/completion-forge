//! OpenAPI 3.0 spec types — delegated to sekkei.

pub use sekkei::*;

// completion-forge-specific extension: iterate operations on a PathItem.
pub trait PathItemExt {
    fn operations(&self) -> Vec<(&str, &sekkei::Operation)>;
}

impl PathItemExt for sekkei::PathItem {
    fn operations(&self) -> Vec<(&str, &sekkei::Operation)> {
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

    #[test]
    fn file_spec_loader_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.yaml");
        std::fs::write(&path, MINIMAL_SPEC).unwrap();

        let loader = FileSpecLoader;
        let spec = loader.load(&path).unwrap();
        assert_eq!(spec.info.title, "Test API");
    }

    #[test]
    fn file_spec_loader_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.json");
        std::fs::write(
            &path,
            r#"{"info":{"title":"JSON API","version":"1.0"},"paths":{}}"#,
        )
        .unwrap();

        let loader = FileSpecLoader;
        let spec = loader.load(&path).unwrap();
        assert_eq!(spec.info.title, "JSON API");
    }

    #[test]
    fn string_spec_loader_yaml() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(MINIMAL_SPEC).unwrap();
        assert_eq!(spec.info.title, "Test API");
    }

    #[test]
    fn string_spec_loader_json() {
        let spec: OpenApiSpec = serde_json::from_str(
            r#"{"info":{"title":"JSON API","version":"1.0"},"paths":{}}"#,
        )
        .unwrap();
        assert_eq!(spec.info.title, "JSON API");
    }
}
