#![allow(clippy::unwrap_used)]
use event_service::{
    domain::Validate,
    scenario::{DeviceCreate, Limits, ReadingCreate},
};
use serde_json::json;
#[test]
fn physical_ranges_required_fields_and_alert_limits_are_distinct() {
    let value = json!({"device_id":"sensor-a","event_id":"e1","occurred_at":"2040-01-01T13:00:00+01:00","temperature":150,"humidity":100,"state":"fault"});
    let reading: ReadingCreate = serde_json::from_value(value.clone()).unwrap();
    assert!(reading.validate().is_ok());
    assert_eq!(
        reading.occurred_at.to_rfc3339(),
        "2040-01-01T12:00:00+00:00"
    );
    for (field, bad) in [
        ("temperature", json!(150.01)),
        ("humidity", json!(-0.1)),
        ("state", json!("unknown")),
        ("device_id", json!("bad/device")),
        ("event_id", json!("")),
    ] {
        let mut input = value.clone();
        input[field] = bad;
        assert!(
            serde_json::from_value::<ReadingCreate>(input)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for field in [
        "event_id",
        "device_id",
        "occurred_at",
        "temperature",
        "humidity",
        "state",
    ] {
        let mut input = value.clone();
        input.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ReadingCreate>(input).is_err());
    }
    let mut reading = reading;
    reading.temperature = f64::NAN;
    assert!(reading.validate().is_err());
    let limits = Limits {
        temperature_min: 0.0,
        temperature_max: 0.0,
        humidity_min: 0.0,
        humidity_max: 100.0,
    };
    assert!(limits.validate().is_err());
    let device: DeviceCreate = serde_json::from_value(json!({"id":"s1","facility_id":"bad/site","limits":{"temperature_min":0,"temperature_max":30,"humidity_min":0,"humidity_max":100}})).unwrap();
    assert!(device.validate().is_err());
}
