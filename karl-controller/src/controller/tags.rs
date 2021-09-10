use std::collections::{HashMap, HashSet};
use itertools::Itertools;
use crate::controller::sensors::Sensor;
use karl_common::*;

type Input = String;
type Output = String;

#[derive(Debug, Clone)]
pub struct Tags {
    node_id: String,
    inputs: HashSet<Input>,
    outputs: HashMap<Output, Vec<Tag>>,
}

impl Tags {
    pub fn new_sensor(sensor: &Sensor) -> Self {
        let inputs = sensor.keys.clone();
        let outputs = sensor.returns.clone();
        Self {
            node_id: sensor.id.clone(),
            inputs: inputs.into_iter().collect(),
            outputs: outputs.into_iter().map(|val| (val, vec![])).collect(),
        }
    }

    pub fn new_module(module: &Module, local_id: String) -> Self {
        let inputs = module.params.clone();
        let outputs = module.returns.clone();
        Self {
            node_id: local_id,
            inputs: inputs.into_iter().collect(),
            outputs: outputs.into_iter().map(|val| (val, vec![])).collect(),
        }
    }

    pub fn contains_input(&self, input: &str) -> bool {
        self.inputs.contains(input)
    }

    pub fn contains_output(&self, output: &str) -> bool {
        self.outputs.contains_key(output)
    }

    pub fn get_output_tags(&self, output: &str) -> Result<&Vec<Tag>, Error> {
        if let Some(tags) = self.outputs.get(output) {
            Ok(tags)
        } else {
            debug!("output {} does not exist", output);
            Err(Error::NotFound)
        }
    }

    pub fn add_output_tag(&mut self, output: &str, tag: &str) -> Result<(), Error> {
        if let Some(tags) = self.outputs.get_mut(output) {
            let tag = tag.to_string();
            if !tags.contains(&tag) {
                tags.push(tag);
            } else {
                warn!("tag {} already exists", tag);
            }
            Ok(())
        } else {
            debug!("output {} does not exist", output);
            Err(Error::NotFound)
        }
    }

    pub fn remove_output_tag(&mut self, output: &str, tag: &str) -> Result<(), Error> {
        if let Some(tags) = self.outputs.get_mut(output) {
            if let Some(index) = tags.iter().position(|t| t == tag) {
                tags.remove(index);
                Ok(())
            } else {
                debug!("tag {} does not exist", tag);
                Err(Error::BadRequest)
            }
        } else {
            debug!("output {} does not exist", output);
            Err(Error::NotFound)
        }
    }

    pub fn inputs_string(&self) -> String {
        let mut inputs: Vec<_> = self.inputs.iter()
            .map(|tag| format!("{}.{}", self.node_id, tag))
            .collect();
        inputs.sort();
        inputs.iter().join(":")
    }

    pub fn outputs_string(&self) -> String {
        let mut outputs: Vec<_> = self.outputs.keys()
            .map(|tag| format!("{}.{}", self.node_id, tag))
            .collect();
        outputs.sort();
        outputs.iter().join(":")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_tags_sensor_constructor() {
        let mut sensor = Sensor::default();
        sensor.keys = vec!["a".to_string(), "b".to_string()];
        sensor.returns = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let tags = Tags::new_sensor(&sensor);
        assert_eq!(tags.inputs.len(), sensor.keys.len());
        assert_eq!(tags.outputs.len(), sensor.returns.len());
    }

    #[test]
    fn test_tags_module_constructor() {
        let mut module = Module::default();
        module.params = vec!["a".to_string(), "b".to_string()];
        module.returns = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let tags = Tags::new_module(&module, "id".to_string());
        assert_eq!(tags.inputs.len(), module.params.len());
        assert_eq!(tags.outputs.len(), module.returns.len());
    }

    fn new_tags() -> Tags {
        let mut module = Module::default();
        module.params = vec!["a".to_string(), "b".to_string()];
        module.returns = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let tags = Tags::new_module(&module, "id".to_string());
        tags
    }

    #[test]
    fn test_contains_input() {
        let tags = new_tags();
        let input_a = "a";
        let input_b = "b";
        let output_x = "x";
        assert!(tags.contains_input(input_a));
        assert!(tags.contains_input(input_b));
        assert!(!tags.contains_input(output_x));
    }

    #[test]
    fn test_read_initial_output_tags() {
        let tags = new_tags();
        let output_x = "x";
        let output_y = "y";
        let output_z = "z";
        let input_a = "a";
        assert!(tags.contains_output(output_x));
        assert!(tags.contains_output(output_y));
        assert!(tags.contains_output(output_z));
        assert!(!tags.contains_output(input_a));
        let tags_z = tags.get_output_tags(output_z);
        assert!(tags_z.is_ok());
        assert!(tags_z.unwrap().is_empty(), "no initial output tags");
        assert!(tags.get_output_tags(input_a).is_err());
    }

    #[test]
    fn test_add_output_tags() {
        let mut tags = new_tags();
        let (input, output) = ("a", "x");
        let expected: Vec<String> = vec![];
        assert_eq!(tags.get_output_tags(output).unwrap(), &expected);
        let expected = vec!["t1".to_string()];
        assert!(tags.add_output_tag(output, "t1").is_ok());
        assert_eq!(tags.get_output_tags(output).unwrap(), &expected, "added tag");
        let expected = vec!["t1".to_string(), "t2".to_string()];
        assert!(tags.add_output_tag(output, "t2").is_ok());
        assert_eq!(tags.get_output_tags(output).unwrap(), &expected, "added tag");
        assert!(tags.add_output_tag(output, "t2").is_ok());
        assert_eq!(tags.get_output_tags(output).unwrap(), &expected,
            "adding same tag represents duplicate edges, don't duplicate internally");
        assert!(tags.add_output_tag(input, "t1").is_err(), "not a valid output");
    }

    #[test]
    fn test_remove_output_tags() {
        let mut tags = new_tags();
        let (input, output) = ("a", "x");
        assert!(tags.add_output_tag(output, "t1").is_ok());
        assert!(tags.add_output_tag(output, "t2").is_ok());
        assert!(tags.add_output_tag(output, "t3").is_ok());
        let expected = vec!["t1".to_string(), "t2".to_string(), "t3".to_string()];
        assert_eq!(tags.get_output_tags(output).unwrap(), &expected);
        let expected = vec!["t1".to_string(), "t3".to_string()];
        assert!(tags.remove_output_tag(output, "t2").is_ok());
        assert_eq!(tags.get_output_tags(output).unwrap(), &expected, "removed t2");
        let expected = vec!["t3".to_string()];
        assert!(tags.remove_output_tag(output, "t1").is_ok());
        assert_eq!(tags.get_output_tags(output).unwrap(), &expected, "removed t1");
        let expected: Vec<String> = vec![];
        assert!(tags.remove_output_tag(output, "t4").is_err(), "invalid tag");
        assert!(tags.remove_output_tag(input, "t3").is_err(), "invalid output");
        assert!(tags.remove_output_tag(output, "t3").is_ok());
        assert_eq!(tags.get_output_tags(output).unwrap(), &expected);
    }

    #[test]
    fn test_inputs_string() {
        let tags = new_tags();
        assert!(!tags.inputs_string().is_empty());
        assert_eq!(tags.inputs_string(), "id.a:id.b");
    }

    #[test]
    fn test_outputs_string() {
        let mut tags = new_tags();
        assert!(!tags.outputs_string().is_empty());
        assert!(tags.add_output_tag("y", "t1").is_ok());
        assert_eq!(tags.outputs_string(), "id.x:id.y:id.z");
        assert!(tags.add_output_tag("y", "t2").is_ok());
        assert_eq!(tags.outputs_string(), "id.x:id.y:id.z");
        assert!(tags.add_output_tag("x", "t3").is_ok());
        assert_eq!(tags.outputs_string(), "id.x:id.y:id.z");
        assert!(tags.add_output_tag("z", "t4").is_ok());
        assert_eq!(tags.outputs_string(), "id.x:id.y:id.z");
        assert!(tags.remove_output_tag("x", "t3").is_ok());
        assert_eq!(tags.outputs_string(), "id.x:id.y:id.z");
    }
}
