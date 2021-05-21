use std::collections::HashMap;
use tonic::{Status, Code};
use itertools::Itertools;
use crate::controller::sensors::Sensor;
use karl_common::*;

type Input = String;
type Output = String;

#[derive(Debug, Clone)]
pub struct Tags {
    is_sensor: bool,
    inputs: HashMap<Input, Option<Tag>>,
    outputs: HashMap<Output, Vec<Tag>>,
}

impl Tags {
    pub fn new_sensor(sensor: &Sensor) -> Self {
        let inputs = sensor.keys.clone();
        let outputs = sensor.returns.clone();
        Self {
            is_sensor: true,
            inputs: inputs.into_iter().map(|val| (val, None)).collect(),
            outputs: outputs.into_iter().map(|val| (val, vec![])).collect(),
        }
    }

    pub fn new_module(module: &Module) -> Self {
        let inputs = module.params.clone();
        let outputs = module.returns.clone();
        Self {
            is_sensor: false,
            inputs: inputs.into_iter().map(|val| (val, None)).collect(),
            outputs: outputs.into_iter().map(|val| (val, vec![])).collect(),
        }
    }

    pub fn contains_input(&self, input: &Input) -> bool {
        self.inputs.contains_key(input)
    }

    pub fn contains_output(&self, output: &Output) -> bool {
        self.outputs.contains_key(output)
    }

    pub fn get_output_tags(&self, output: &Output) -> Result<&Vec<Tag>, Status> {
        if let Some(tags) = self.outputs.get(output) {
            Ok(tags)
        } else {
            Err(Status::new(Code::NotFound, format!("output {} does not exist", output)))
        }
    }

    pub fn add_output_tag(&mut self, output: &Output, tag: &Tag) -> Result<(), Status> {
        if let Some(tags) = self.outputs.get_mut(output) {
            if !tags.contains(tag) {
                tags.push(tag.to_string());
                Ok(())
            } else {
                Err(Status::new(Code::AlreadyExists, format!("tag {} already exists", tag)))
            }
        } else {
            Err(Status::new(Code::NotFound, format!("output {} does not exist", output)))
        }
    }

    pub fn get_input_tag(&self, input: &Output) -> Result<&Option<Tag>, Status> {
        if let Some(tag) = self.inputs.get(input) {
            Ok(tag)
        } else {
            Err(Status::new(Code::NotFound, format!("input {} does not exist", input)))
        }
    }

    pub fn set_input_tag(&mut self, input: &Output, tag: &Tag) -> Result<(), Status> {
        if let Some(current_tag) = self.inputs.get_mut(input) {
            if current_tag.is_some() {
                warn!("replacing current tag {:?} with {}", current_tag, tag);
            }
            *current_tag = Some(tag.to_string());
            Ok(())
        } else {
            Err(Status::new(Code::NotFound, format!("input {} does not exist", input)))
        }
    }

    pub fn inputs_string(&self) -> String {
        self.inputs.iter()
            .filter(|(_, tag)| tag.is_some())
            .map(|(input, tag)| format!("{};{}", input, tag.as_ref().unwrap()))
            .join(":")
    }

    pub fn outputs_string(&self) -> String {
        self.outputs.iter()
            .filter(|(_, tags)| !tags.is_empty())
            .map(|(output, tags)| format!("{};{}", output, tags.iter().join(",")))
            .join(":")
    }
}

pub fn is_state_tag(tag: &Tag) -> bool {
    tag.chars().next() == Some('#')
}

pub fn parse_state_tag(tag: &Tag) -> (String, String) {
    let mut split = tag.split(".");
    let sensor = split.next().unwrap()[1..].to_string();
    let key = split.next().unwrap().to_string();
    (sensor, key)
}

pub fn to_state_tag(sensor: &str, key: &str) -> String {
    format!("#{}.{}", sensor, key)
}
