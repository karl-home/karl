use std::collections::HashMap;
use crate::controller::tags::Tags;
use karl_common::Error;

type SensorID = String;
type SensorKey = String;
type SensorReturn = String;
type SensorToken = String;

#[derive(Serialize, Debug, Clone, Default)]
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

    pub fn add_sensor(
        &mut self,
        sensor: Sensor,
        token: SensorToken,
    ) -> Result<(), Error> {
        if self.tokens.contains_key(&token) {
            return Err(Error::BadRequestInfo("duplicate token".to_string()));
        }
        if self.values.contains_key(&sensor.id) {
            return Err(Error::BadRequestInfo("duplicate sensor id".to_string()));
        }
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
        Ok(())
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

#[cfg(test)]
mod test {
    use super::*;
    use karl_common::Token;

    fn camera(id: SensorID) -> Sensor {
        Sensor {
            confirmed: true,
            id,
            keys: vec!["a".to_string()],
            returns: vec!["x".to_string(), "y".to_string()],
        }
    }

    #[test]
    fn test_add_sensor_basic() {
        let mut s = Sensors::default();
        let id = "camera".to_string();
        assert!(s.get_sensor(&id).is_none());
        assert!(s.list_sensors().is_empty());
        assert!(!s.sensor_exists(&id));
        assert!(s.tags(&id).is_err());
        assert!(s.tags_mut(&id).is_err());

        let sensor = camera(id.clone());
        assert!(s.add_sensor(sensor, Token::gen()).is_ok());
        assert!(s.get_sensor(&id).is_some());
        assert_eq!(s.list_sensors().len(), 1);
        assert!(s.sensor_exists(&id));
        assert!(s.tags(&id).is_ok());
        assert!(s.tags_mut(&id).is_ok());
    }

    #[test]
    fn test_add_sensor_error() {
        let mut s = Sensors::default();
        let sensor1 = camera("camera".to_string());
        let sensor2 = camera("camera_1".to_string());
        let token = Token::gen();
        assert!(s.add_sensor(sensor1, token.clone()).is_ok(), "first sensor");
        assert!(s.add_sensor(sensor2.clone(), token.clone()).is_err(), "duplicate token");
        assert!(s.add_sensor(sensor2.clone(), Token::gen()).is_ok());
        assert!(s.add_sensor(sensor2.clone(), Token::gen()).is_err(), "duplicate sensor id");
    }

    #[test]
    fn test_authenticate_and_confirm_sensor() {
        let mut s = Sensors::default();
        let token1 = Token::gen();
        let token2 = Token::gen();
        let token3 = Token::gen();
        let confirmed_sensor = camera("camera".to_string());
        let mut unconfirmed_sensor = camera("camera_1".to_string());
        unconfirmed_sensor.confirmed = false;
        assert!(s.add_sensor(confirmed_sensor, token1.clone()).is_ok());
        assert!(s.add_sensor(unconfirmed_sensor, token2.clone()).is_ok());
        assert_eq!(s.authenticate(&token1), Some(&"camera".to_string()));
        assert_eq!(s.authenticate(&token3), None, "invalid token");
        assert_eq!(s.authenticate(&token2), None, "unconfirmed sensor");

        s.confirm_sensor(&"camera_2".to_string());  // no effect on invalid id
        assert_eq!(s.authenticate(&token2), None);
        s.confirm_sensor(&"camera".to_string());  // no effect if already confirmed
        assert_eq!(s.authenticate(&token2), None);
        s.confirm_sensor(&"camera_1".to_string());
        assert_eq!(s.authenticate(&token2), Some(&"camera_1".to_string()));
    }

    #[test]
    fn test_remove_sensors() {
        let mut s = Sensors::default();
        let id1 = "camera".to_string();
        let id2 = "camera_1".to_string();
        let confirmed_sensor = camera(id1.clone());
        let mut unconfirmed_sensor = camera(id2.clone());
        unconfirmed_sensor.confirmed = false;
        assert!(s.add_sensor(confirmed_sensor, Token::gen()).is_ok());
        assert!(s.add_sensor(unconfirmed_sensor, Token::gen()).is_ok());
        assert_eq!(s.list_sensors().len(), 2);
        assert!(s.remove_sensor(&id2).is_some(), "removed unconfirmed sensor");
        assert_eq!(s.list_sensors().len(), 1);
        assert!(s.remove_sensor(&id1).is_some(), "removed confirmed sensor");
        assert_eq!(s.list_sensors().len(), 0);
        assert!(s.remove_sensor(&id1).is_none(), "no sensor to remove");
    }

    #[test]
    fn test_unique_id_generator() {
        let mut s = Sensors::default();
        let id = "camera".to_string();
        assert_eq!(s.unique_id(id.clone()), id, "id is preserved");
        assert_eq!(s.unique_id(id.clone()), id, "id has not been added yet");
        assert!(s.add_sensor(camera("camera".to_string()), Token::gen()).is_ok());
        assert_eq!(s.unique_id(id.clone()), "camera_1".to_string());
        let mut unconfirmed_sensor = camera("camera_1".to_string());
        unconfirmed_sensor.confirmed = false;
        assert!(s.add_sensor(unconfirmed_sensor, Token::gen()).is_ok());
        assert_eq!(s.unique_id(id.clone()), "camera_2".to_string());
        assert!(s.add_sensor(camera("camera_2".to_string()), Token::gen()).is_ok());
        assert_eq!(s.unique_id("camera_2".to_string()), "camera_2_1".to_string());
    }
}
