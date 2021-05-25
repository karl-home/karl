pub fn is_state_tag(tag: &str) -> bool {
    if tag.chars().next() != Some('#') {
        return false;
    }
    let mut split = tag.split(".");
    split.next().is_some() && split.next().is_some()
}

pub fn parse_state_tag(tag: &str) -> (String, String) {
    let mut split = tag.split(".");
    let sensor = split.next().unwrap()[1..].to_string();
    let key = split.next().unwrap().to_string();
    (sensor, key)
}

pub fn to_state_tag(sensor: &str, key: &str) -> String {
    format!("#{}.{}", sensor, key)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_state_tag_validation() {
        assert!(is_state_tag("#sensor.key"));
        assert!(is_state_tag("#sensor_1.key"), "valid non-alphabetic characters");
        assert!(!is_state_tag("sensor.key"), "no starting #");
        assert!(!is_state_tag("#sensor"), "missing period-delimited component");
    }

    #[test]
    fn test_parse_state_tag() {
        let (sensor, key) = parse_state_tag("#sensor.key");
        assert_eq!(&sensor, "sensor");
        assert_eq!(&key, "key");
        let (sensor, key) = parse_state_tag("#sensor_1.key_1");
        assert_eq!(&sensor, "sensor_1");
        assert_eq!(&key, "key_1");
    }

    #[test]
    fn test_to_state_tag() {
        assert_eq!(&to_state_tag("sensor", "key"), "#sensor.key");
        assert_eq!(&to_state_tag("sensor_1", "key_1"), "#sensor_1.key_1");
    }
}