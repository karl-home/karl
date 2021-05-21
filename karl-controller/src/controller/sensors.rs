use std::collections::HashMap;
use crate::controller::tags::Tags;
use karl_common::Error;

type SensorID = String;
type SensorKey = String;
type SensorReturn = String;
type SensorToken = String;

#[derive(Serialize, Debug, Clone)]
pub struct Sensor {
    pub confirmed: bool,
    pub id: SensorID,
    pub keys: Vec<SensorKey>,
    pub returns: Vec<SensorReturn>,
}

#[derive(Default, Clone)]
pub struct Sensors {
    tokens: HashMap<SensorToken, SensorID>,
    values: HashMap<SensorID, Sensor>,
    tags_inner: HashMap<SensorID, Tags>,
}

impl Sensors {
    pub fn authenticate(&self, token: &SensorToken) -> Option<&SensorID> {
        if let Some(id) = self.tokens.get(token) {
            if self.values.get(id).unwrap().confirmed {
                Some(id)
            } else {
                debug!("unconfirmed sensor {}", id);
                None
            }
        } else {
            debug!("invalid sensor token {}", token);
            None
        }
    }

    pub fn unique_id(&self, mut id: SensorID) -> SensorID {
        id = id.trim().to_lowercase();
        id = id
            .chars()
            .filter(|ch| ch.is_alphanumeric() || ch == &'_')
            .collect();
        if self.values.contains_key(&id) {
            let mut i = 1;
            loop {
                let new_id = format!("{}_{}", id, i);
                if !self.values.contains_key(&new_id) {
                    id = new_id;
                    break;
                }
                i += 1;
            }
        }
        id
    }

    pub fn add_sensor(&mut self, sensor: Sensor, token: SensorToken) {
        assert!(!self.tokens.contains_key(&token), "duplicate token");
        assert!(!self.values.contains_key(&sensor.id), "duplicate sensor id");
        assert!(!self.tags_inner.contains_key(&sensor.id), "inconsistent state");
        info!(
            "registered sensor {} {} keys={:?} returns={:?}",
            sensor.id,
            token,
            sensor.keys,
            sensor.returns,
        );
        self.tokens.insert(token, sensor.id.clone());
        self.tags_inner.insert(sensor.id.clone(), Tags::new_sensor(&sensor));
        self.values.insert(sensor.id.clone(), sensor);
    }

    // Make sure to update any other state that references the sensor!
    pub fn remove_sensor(&mut self, id: &SensorID) -> Option<Sensor> {
        info!("removed sensor {}", id);
        let mut tokens = self.tokens.iter()
            .filter(|(_, sensor_id)| sensor_id == &id)
            .map(|(token, _)| token.clone());
        if let Some(token) = tokens.next() {
            self.tokens.remove(&token).unwrap();
            self.tags_inner.remove(id).unwrap();
            Some(self.values.remove(id).unwrap())
        } else {
            None
        }
    }

    pub fn get_sensor(&self, id: &SensorID) -> Option<&Sensor> {
        self.values.get(id)
    }

    pub fn list_sensors(&self) -> Vec<&Sensor> {
        self.values.values().collect()
    }

    pub fn confirm_sensor(&mut self, id: &SensorID) {
        if let Some(sensor) = self.values.get_mut(id) {
            if sensor.confirmed {
                warn!("attempted to confirm already confirmed sensor: {:?}", id);
            } else {
                info!("confirmed sensor {}", id);
                sensor.confirmed = true;
            }
        } else {
            warn!("attempted to confirm nonexistent sensor: {:?}", id);
        }
    }

    pub fn sensor_exists(&self, id: &SensorID) -> bool {
        self.values.contains_key(id)
    }

    pub fn tags(&self, id: &SensorID) -> Result<&Tags, Error> {
        if let Some(tags) = self.tags_inner.get(id) {
            Ok(tags)
        } else {
            debug!("sensor {} does not exist", id);
            Err(Error::NotFound)
        }
    }

    pub fn tags_mut(&mut self, id: &SensorID) -> Result<&mut Tags, Error> {
        if let Some(tags) = self.tags_inner.get_mut(id) {
            Ok(tags)
        } else {
            debug!("sensor {} does not exist", id);
            Err(Error::NotFound)
        }
    }
}
