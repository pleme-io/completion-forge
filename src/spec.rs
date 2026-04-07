//! `OpenAPI` 3.0 spec types — delegated to sekkei.

pub use sekkei::*;

/// Extension trait for iterating HTTP operations on a `PathItem`.
pub trait PathItemExt {
    /// Returns a list of `(HTTP_METHOD, Operation)` pairs present on this path item.
    fn operations(&self) -> Vec<(&str, &sekkei::Operation)>;
}

impl PathItemExt for sekkei::PathItem {
    fn operations(&self) -> Vec<(&str, &sekkei::Operation)> {
        [
            ("GET", &self.get),
            ("POST", &self.post),
            ("PUT", &self.put),
            ("DELETE", &self.delete),
            ("PATCH", &self.patch),
        ]
        .into_iter()
        .filter_map(|(method, opt)| opt.as_ref().map(|op| (method, op)))
        .collect()
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

    #[test]
    fn path_item_operations_put_patch_delete() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: Full Methods API
  version: "1.0.0"
paths:
  /resources/{id}:
    put:
      operationId: replaceResource
      summary: Replace resource
    patch:
      operationId: updateResource
      summary: Update resource
    delete:
      operationId: deleteResource
      summary: Delete resource
"#,
        )
        .unwrap();
        let item = &spec.paths["/resources/{id}"];
        let ops = item.operations();
        assert_eq!(ops.len(), 3);
        let methods: Vec<&str> = ops.iter().map(|o| o.0).collect();
        assert!(methods.contains(&"PUT"));
        assert!(methods.contains(&"PATCH"));
        assert!(methods.contains(&"DELETE"));
    }

    #[test]
    fn path_item_operations_all_methods() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: All Methods
  version: "1.0.0"
paths:
  /things:
    get:
      summary: Get
    post:
      summary: Create
    put:
      summary: Replace
    delete:
      summary: Remove
    patch:
      summary: Patch
"#,
        )
        .unwrap();
        let item = &spec.paths["/things"];
        let ops = item.operations();
        assert_eq!(ops.len(), 5);
        assert_eq!(ops[0].0, "GET");
        assert_eq!(ops[1].0, "POST");
        assert_eq!(ops[2].0, "PUT");
        assert_eq!(ops[3].0, "DELETE");
        assert_eq!(ops[4].0, "PATCH");
    }

    #[test]
    fn path_item_operations_empty() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(
            r#"
info:
  title: Empty Paths
  version: "1.0.0"
paths:
  /empty:
    parameters: []
"#,
        )
        .unwrap();
        let item = &spec.paths["/empty"];
        let ops = item.operations();
        assert!(ops.is_empty());
    }

    #[test]
    fn file_spec_loader_missing_file() {
        let loader = FileSpecLoader;
        let result = loader.load(std::path::Path::new("/nonexistent/file.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn file_spec_loader_invalid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, "not: [valid: yaml: {{{").unwrap();

        let loader = FileSpecLoader;
        let result = loader.load(&path);
        assert!(result.is_err());
    }

    #[test]
    fn file_spec_loader_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{not valid json}").unwrap();

        let loader = FileSpecLoader;
        let result = loader.load(&path);
        assert!(result.is_err());
    }

    #[test]
    fn parse_spec_with_description() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(PETSTORE_SPEC).unwrap();
        assert_eq!(
            spec.info.description.as_deref(),
            Some("A sample pet store API")
        );
    }

    #[test]
    fn parse_spec_no_description() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(MINIMAL_SPEC).unwrap();
        assert!(spec.info.description.is_none());
    }

    #[test]
    fn parse_path_level_parameters() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(PETSTORE_SPEC).unwrap();
        let pet_by_id = &spec.paths["/pets/{petId}"];
        assert_eq!(pet_by_id.parameters.len(), 1);
        assert_eq!(pet_by_id.parameters[0].name, "petId");
        assert_eq!(pet_by_id.parameters[0].location, "path");
        assert!(pet_by_id.parameters[0].required);
    }

    #[test]
    fn parse_operation_without_parameters() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(PETSTORE_SPEC).unwrap();
        let delete_op = spec.paths["/pets/{petId}"].delete.as_ref().unwrap();
        assert!(delete_op.parameters.is_empty());
    }

    #[test]
    fn parse_multiple_paths() {
        let spec: OpenApiSpec = serde_yaml_ng::from_str(PETSTORE_SPEC).unwrap();
        assert!(spec.paths.contains_key("/pets"));
        assert!(spec.paths.contains_key("/pets/{petId}"));
        assert!(spec.paths.contains_key("/stores"));
    }
}
