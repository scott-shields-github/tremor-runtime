// Copyright 2020-2021, The Tremor Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// DogStatsd Protocol v1.2 - https://docs.datadoghq.com/developers/dogstatsd/datagram_shell/
//
// Examples
//
// Metric
// <METRIC_NAME>:<VALUE1>:<VALUE2>:<VALUE3>|<TYPE>|@<SAMPLE_RATE>|#<TAG_KEY_1>:<TAG_VALUE_1>,<TAG_2>|c:<CONTAINER_ID>
//
// Event
// _e{<TITLE_UTF8_LENGTH>,<TEXT_UTF8_LENGTH>}:<TITLE>|<TEXT>|d:<TIMESTAMP>|h:<HOSTNAME>|p:<PRIORITY>|t:<ALERT_TYPE>|#<TAG_KEY_1>:<TAG_VALUE_1>,<TAG_2>
//
// Service Check
// _sc|<NAME>|<STATUS>|d:<TIMESTAMP>|h:<HOSTNAME>|#<TAG_KEY_1>:<TAG_VALUE_1>,<TAG_2>|m:<SERVICE_CHECK_MESSAGE>

use super::prelude::*;
use std::{slice::SliceIndex, str};

#[derive(Clone)]
pub struct DogStatsD {}

impl Codec for DogStatsD {
    fn name(&self) -> &str {
        "dogstatsd"
    }

    fn decode<'input>(
        &mut self,
        data: &'input mut [u8],
        ingest_ns: u64,
    ) -> Result<Option<Value<'input>>> {
        decode(data, ingest_ns).map(Some)
    }

    fn encode(&self, data: &Value) -> Result<Vec<u8>> {
        let dogstatsd_type = data
            .get_str("dogstatsd_type")
            .ok_or(ErrorKind::InvalidDogStatsD)?;
        match dogstatsd_type {
            "metric" => encode_metric(data),
            "event" => encode_event(data),
            "service_check" => encode_service_check(data),
            _ => Err(ErrorKind::InvalidDogStatsD.into()),
        }
    }

    fn boxed_clone(&self) -> Box<dyn Codec> {
        Box::new(self.clone())
    }
}

fn encode_metric(value: &Value) -> Result<Vec<u8>> {
    let mut r = String::new();
    r.push_str(value.get_str("metric").ok_or(ErrorKind::InvalidDogStatsD)?);
    let t = value.get_str("type").ok_or(ErrorKind::InvalidDogStatsD)?;
    let values = value
        .get_array("values")
        .ok_or(ErrorKind::InvalidDogStatsD)?;

    let value_array: Vec<String> = values
        .iter()
        .map(|x| {
            let n = x.as_f64().unwrap();
            if n.fract() == 0.0 {
                let i = n as i32;
                i.to_string();
            }
            n.to_string()
        })
        .collect();

    r.push(':');

    r.push_str(&value_array.join(":"));
    r.push('|');
    r.push_str(t);

    if let Some(val) = value.get("sample_rate") {
        if val.is_number() {
            r.push_str("|@");
            r.push_str(&val.encode());
        } else {
            return Err(ErrorKind::InvalidDogStatsD.into());
        }
    }

    if let Some(tags) = value.get_array("tags") {
        r.push_str("|#");
        let tag_array: Vec<&str> = tags.iter().map(|tag| tag.as_str().unwrap()).collect();
        r.push_str(&tag_array.join(","))
    }

    if let Some(container_id) = value.get_str("container_id") {
        r.push_str("|c:");
        r.push_str(container_id);
    }

    Ok(r.as_bytes().to_vec())
}

fn encode_event(value: &Value) -> Result<Vec<u8>> {
    let mut r = String::new();
    let title = value.get_str("title").ok_or(ErrorKind::InvalidDogStatsD)?;
    let title_length = value
        .get_i32("title_length")
        .ok_or(ErrorKind::InvalidDogStatsD)?;
    let text = value.get_str("text").ok_or(ErrorKind::InvalidDogStatsD)?;
    let text_length = value
        .get_i32("text_length")
        .ok_or(ErrorKind::InvalidDogStatsD)?;

    r.push_str("_e{");
    r.push_str(&title_length.to_string());
    r.push(',');
    r.push_str(&text_length.to_string());
    r.push_str("}:");
    r.push_str(title);
    r.push('|');
    r.push_str(text);

    if let Some(timestamp) = value.get_u32("timestamp") {
        r.push_str("|d:");
        r.push_str(&timestamp.to_string());
    }

    if let Some(hostname) = value.get_str("hostname") {
        r.push_str("|h:");
        r.push_str(hostname);
    }

    if let Some(aggregation_key) = value.get_str("aggregation_key") {
        r.push_str("|k:");
        r.push_str(aggregation_key);
    }

    if let Some(priority) = value.get_str("priority") {
        r.push_str("|p:");
        r.push_str(priority);
    }

    if let Some(source) = value.get_str("source") {
        r.push_str("|s:");
        r.push_str(source);
    }

    if let Some(dogstatsd_type) = value.get_str("type") {
        r.push_str("|t:");
        r.push_str(dogstatsd_type);
    }

    if let Some(tags) = value.get_array("tags") {
        r.push_str("|#");
        let tag_array: Vec<&str> = tags.iter().map(|tag| tag.as_str().unwrap()).collect();
        r.push_str(&tag_array.join(","))
    }

    if let Some(container_id) = value.get_str("container_id") {
        r.push_str("|c:");
        r.push_str(container_id);
    }

    Ok(r.as_bytes().to_vec())
}

fn encode_service_check(value: &Value) -> Result<Vec<u8>> {
    let mut r = String::new();
    let name = value.get_str("name").ok_or(ErrorKind::InvalidDogStatsD)?;
    let status = value.get_i32("status").ok_or(ErrorKind::InvalidDogStatsD)?;

    r.push_str("_sc|");
    r.push_str(name);
    r.push('|');
    r.push_str(&status.to_string());

    if let Some(timestamp) = value.get_u32("timestamp") {
        r.push_str("|d:");
        r.push_str(&timestamp.to_string());
    }

    if let Some(hostname) = value.get_str("hostname") {
        r.push_str("|h:");
        r.push_str(hostname);
    }

    if let Some(tags) = value.get_array("tags") {
        r.push_str("|#");
        let tag_array: Vec<&str> = tags.iter().map(|tag| tag.as_str().unwrap()).collect();
        r.push_str(&tag_array.join(","))
    }

    if let Some(message) = value.get_str("message") {
        r.push_str("|m:");
        r.push_str(message);
    }

    if let Some(container_id) = value.get_str("container_id") {
        r.push_str("|c:");
        r.push_str(container_id);
    }

    Ok(r.as_bytes().to_vec())
}

fn decode(data: &[u8], _ingest_ns: u64) -> Result<Value> {
    let first_bytes = data.get(0..2).ok_or_else(invalid)?;
    let first_chars = str::from_utf8(first_bytes)?;

    match first_chars {
        // Event
        "_e" => decode_event(data),
        "_s" => decode_service_check(data),
        _ => decode_metric(data),
    }
}

fn decode_metric(data: &[u8]) -> Result<Value> {
    let mut d = data.iter().enumerate();
    let mut m = Object::with_capacity(7);
    m.insert("dogstatsd_type".into(), Value::from("metric"));
    let mut section_start: usize;

    loop {
        match d.next() {
            // <METRIC_NAME>
            Some((idx, b':')) => {
                let v = substr(data, 0..idx)?;
                section_start = idx + 1;
                m.insert("metric".into(), Value::from(v));
                break;
            }
            Some(_) => (),
            None => return Err(invalid()),
        }
    }

    // Value(s) - <VALUE1>:<VALUE2>
    let mut values = Vec::new();
    loop {
        match d.next() {
            Some((idx, b':' | b'|')) => {
                let s = substr(data, section_start..idx)?;
                let v: f64 = s.parse()?;
                let value = Value::from(v);
                values.push(value);
                section_start = idx + 1;

                if substr(data, idx..=idx)?.eq("|") {
                    break;
                }
            }
            Some(_) => (),
            None => return Err(invalid()),
        }
    }
    m.insert("values".into(), Value::from(values));

    // <TYPE>
    match d.next() {
        Some((i, b'c' | b'd' | b'g' | b'h' | b's')) => {
            section_start = i + 1;
            m.insert("type".into(), substr(data, i..=i)?.into());
        }
        Some((i, b'm')) => {
            if let Some((j, b's')) = d.next() {
                m.insert("type".into(), substr(data, i..=j)?.into());
                section_start = i + 1;
            } else {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    };

    // Optional Sections
    let sections: Vec<&str> = substr(data, section_start..)?.split("|").collect();

    for section in sections.iter() {
        if section.starts_with('@') {
            let sample_rate = &section[1..];
            let sample_rate_float: f64 = sample_rate.parse()?;
            m.insert("sample_rate".into(), Value::from(sample_rate_float));
        } else if section.starts_with('#') {
            let tags: Vec<&str> = section[1..].split(",").collect();
            m.insert("tags".into(), Value::from(tags));
        } else if section.starts_with('c') {
            let container_id = &section[2..];
            m.insert("container_id".into(), Value::from(container_id));
        }
    }

    Ok(Value::from(m))
}

fn decode_event(data: &[u8]) -> Result<Value> {
    let mut d = data.iter().enumerate();
    let mut m = Object::with_capacity(13);
    m.insert("dogstatsd_type".into(), Value::from("event"));
    let section_start: usize;
    let mut optional_sections = false;
    let mut optional_text_idx = 0;

    // Title/Text Lengths and Title
    loop {
        match d.next() {
            Some((idx, b'|')) => {
                let v: Vec<&str> = substr(data, 2..idx)?.split(":").collect();
                let lens = v[0];
                let len_vec: Vec<&str> = lens.split(",").collect();
                let title_len: i32 = len_vec[0][1..].parse().unwrap();
                let text_len: i32 = len_vec[1][0..len_vec[1].len() - 1].parse().unwrap();
                let title = v[1];
                m.insert("title_length".into(), Value::from(title_len));
                m.insert("text_length".into(), Value::from(text_len));
                m.insert("title".into(), Value::from(title));
                section_start = idx + 1;
                break;
            }
            Some(_) => (),
            None => return Err(invalid()),
        }
    }

    // Text
    loop {
        match d.next() {
            Some((idx, _)) => {
                let mut is_end = false;
                let mut text_end_index = 0;
                if idx == data.len() - 1 {
                    is_end = true;
                    text_end_index = idx;
                } else if substr(data, idx..=idx)?.eq("|") {
                    is_end = true;
                    text_end_index = idx - 1;
                    optional_sections = true;
                    optional_text_idx = idx + 1;
                }
                if is_end && text_end_index > 0 {
                    let text = substr(data, section_start..=text_end_index)?;
                    m.insert("text".into(), Value::from(text));
                    break;
                }
            }
            None => return Err(invalid()),
        }
    }

    // Optional Sections
    if optional_sections {
        let sections: Vec<&str> = substr(data, optional_text_idx..)?.split("|").collect();

        for section in sections.iter() {
            if section.starts_with('d') {
                let timestamp: u32 = section[2..].parse()?;
                m.insert("timestamp".into(), Value::from(timestamp));
            } else if section.starts_with('h') {
                let hostname = &section[2..];
                m.insert("hostname".into(), Value::from(hostname));
            } else if section.starts_with('p') {
                let priority = &section[2..];
                m.insert("priority".into(), Value::from(priority));
            } else if section.starts_with('s') {
                let source = &section[2..];
                m.insert("source".into(), Value::from(source));
            } else if section.starts_with('t') {
                let event_type = &section[2..];
                m.insert("type".into(), Value::from(event_type));
            } else if section.starts_with('k') {
                let aggregation = &section[2..];
                m.insert("aggregation_key".into(), Value::from(aggregation));
            } else if section.starts_with('#') {
                let tags: Vec<&str> = section[1..].split(",").collect();
                m.insert("tags".into(), Value::from(tags));
            } else if section.starts_with('c') {
                let container_id = &section[2..];
                m.insert("container_id".into(), Value::from(container_id));
            }
        }
    }

    Ok(Value::from(m))
}

fn decode_service_check(data: &[u8]) -> Result<Value> {
    let mut d = data.iter().enumerate();
    let mut m = Object::with_capacity(8);
    m.insert("dogstatsd_type".into(), Value::from("service_check"));
    let start_index: usize;

    // Skip the prefix and set the starting
    loop {
        match d.next() {
            Some((idx, b'|')) => {
                start_index = idx + 1;
                break;
            }
            _ => (),
        }
    }

    // Name
    loop {
        match d.next() {
            Some((idx, b'|')) => {
                let name = substr(data, start_index..idx)?;
                m.insert("name".into(), Value::from(name));
                break;
            }
            Some(_) => (),
            None => return Err(invalid()),
        }
    }

    // Status
    match d.next() {
        Some((idx, b'0' | b'1' | b'2' | b'3')) => {
            let status_str = substr(data, idx..=idx)?;
            let status: i32 = status_str.parse()?;
            m.insert("status".into(), Value::from(status));
        }
        _ => return Err(invalid()),
    }

    // Optional Sections
    match d.next() {
        Some((idx, b'|')) => {
            let sections: Vec<&str> = substr(data, idx + 1..)?.split("|").collect();
            for section in sections.iter() {
                if section.starts_with('d') {
                    let timestamp: u32 = section[2..].parse()?;
                    m.insert("timestamp".into(), Value::from(timestamp));
                } else if section.starts_with('h') {
                    let hostname = &section[2..];
                    m.insert("hostname".into(), Value::from(hostname));
                } else if section.starts_with('#') {
                    let tags: Vec<&str> = section[1..].split(",").collect();
                    m.insert("tags".into(), Value::from(tags));
                } else if section.starts_with('m') {
                    let message = &section[2..];
                    m.insert("message".into(), Value::from(message));
                } else if section.starts_with('c') {
                    let container_id = &section[2..];
                    m.insert("container_id".into(), Value::from(container_id));
                }
            }
        }
        Some(_) => return Err(invalid()),
        None => (),
    }

    Ok(Value::from(m))
}

fn invalid() -> Error {
    Error::from(ErrorKind::InvalidDogStatsD)
}

fn substr<I: SliceIndex<[u8], Output = [u8]>>(data: &[u8], r: I) -> Result<&str> {
    let raw = data.get(r).ok_or_else(invalid)?;
    let s = str::from_utf8(raw)?;
    Ok(s)
}

#[cfg(test)]
mod test {
    use super::*;
    use tremor_value::literal;
    #[test]
    fn test_subslice() {
        let a = b"012345";

        assert_eq!(substr(a, 1..), Ok("12345"));
        assert_eq!(substr(a, ..4), Ok("0123"));
        assert_eq!(substr(a, 1..4), Ok("123"));
        assert!(substr(a, 99..).is_err());
        assert!(substr(a, ..99).is_err());
    }

    #[test]
    fn dogstatsd_complete_payload() {
        let data = b"dog:111|g|@0.5|#foo:bar,fizz:buzz|c:123abc";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "metric",
            "metric": "dog",
            "values": [111.],
            "type": "g",
            "sample_rate": 0.5,
            "tags": ["foo:bar", "fizz:buzz"],
            "container_id": "123abc",
        });
        assert_eq!(parsed, expected);
        let encoded = encode_metric(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_complete_payload_multiple_values() {
        let data = b"dog:111:222:333:4.44|g|@0.5|#foo:bar,fizz:buzz|c:123abc";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "metric",
            "metric": "dog",
            "values": [111.,222.,333.,4.44],
            "type": "g",
            "sample_rate": 0.5,
            "tags": ["foo:bar", "fizz:buzz"],
            "container_id": "123abc",
        });
        assert_eq!(parsed, expected);
        let encoded = encode_metric(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_payload_with_sample_and_tags() {
        let data = b"dog:111|g|@0.5|#foo:bar,fizz:buzz";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "metric",
            "metric": "dog",
            "values": [111 as f64],
            "type": "g",
            "sample_rate": 0.5,
            "tags": ["foo:bar", "fizz:buzz"],
        });
        assert_eq!(parsed, expected);
        let encoded = encode_metric(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_payload_with_sample_and_container_id() {
        let data = b"dog:111|g|@0.5|c:123abc";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "metric",
            "metric": "dog",
            "values": [111.],
            "type": "g",
            "sample_rate": 0.5,
            "container_id": "123abc",
        });
        assert_eq!(parsed, expected);
        let encoded = encode_metric(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_payload_with_tags_and_container_id() {
        let data = b"dog:111|g|#foo:bar,fizz:buzz|c:123abc";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "metric",
            "metric": "dog",
            "values": [111.],
            "type": "g",
            "tags": ["foo:bar", "fizz:buzz"],
            "container_id": "123abc",
        });
        assert_eq!(parsed, expected);
        let encoded = encode_metric(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_payload_with_tags() {
        let data = b"dog:111|g|#foo:bar,fizz:buzz";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "metric",
            "metric": "dog",
            "values": [111.],
            "type": "g",
            "tags": ["foo:bar", "fizz:buzz"],
        });
        assert_eq!(parsed, expected);
        let encoded = encode_metric(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_payload_with_tag() {
        let data = b"dog:111|g|#foo:bar";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "metric",
            "metric": "dog",
            "values": [111.],
            "type": "g",
            "tags": ["foo:bar"],
        });
        assert_eq!(parsed, expected);
        let encoded = encode_metric(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_payload_with_container_id() {
        let data = b"dog:111|g|c:123abc";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "metric",
            "metric": "dog",
            "values": [111.],
            "type": "g",
            "container_id": "123abc",
        });
        assert_eq!(parsed, expected);
        let encoded = encode_metric(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_count() {
        let data = b"dog:1|c";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "metric",
            "type": "c",
            "metric": "dog",
            "values": [1.],

        });
        assert_eq!(parsed, expected);
        let encoded = encode_metric(&parsed).expect("failed to encode");
        assert_eq!(encoded.as_slice(), data);
    }

    #[test]
    fn dogstatsd_time() {
        let data = b"dog:320|ms";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "metric",
            "type": "ms",
            "metric": "dog",
            "values": [320.],

        });
        assert_eq!(parsed, expected);
        let encoded = encode_metric(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_event() {
        let data = b"_e{21,36}:An exception occurred|Cannot parse CSV file from 10.0.0.17|t:warning|#err_type:bad_file";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "event",
            "title_length": 21,
            "text_length": 36,
            "title": "An exception occurred",
            "text": "Cannot parse CSV file from 10.0.0.17",
            "type": "warning",
            "tags": ["err_type:bad_file"],
        });
        assert_eq!(parsed, expected);
        let encoded = encode_event(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_basic_event() {
        let data = b"_e{21,36}:An exception occurred|Cannot parse CSV file from 10.0.0.17";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "event",
            "title_length": 21,
            "text_length": 36,
            "title": "An exception occurred",
            "text": "Cannot parse CSV file from 10.0.0.17",
        });
        assert_eq!(parsed, expected);
        let encoded = encode_event(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_complete_event() {
        let data = b"_e{4,6}:Test|A Test|d:1663016695|h:test.example.com|k:a1b2c3|p:normal|s:test|t:warning|#err_type:bad_file|c:123abc";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "event",
            "title_length": 4,
            "text_length": 6,
            "title": "Test",
            "text": "A Test",
            "timestamp": 1663016695 as u32,
            "hostname": "test.example.com",
            "aggregation_key": "a1b2c3",
            "priority": "normal",
            "source": "test",
            "type": "warning",
            "tags": ["err_type:bad_file"],
            "container_id": "123abc",
        });
        assert_eq!(parsed, expected);
        let encoded = encode_event(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_service_check() {
        let data = b"_sc|Redis connection|2|#env:dev|m:Redis connection timed out after 10s";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "service_check",
            "name": "Redis connection",
            "status": 2,
            "tags": ["env:dev"],
            "message": "Redis connection timed out after 10s",
        });
        assert_eq!(parsed, expected);
        let encoded = encode_service_check(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_basic_service_check() {
        let data = b"_sc|Redis connection|2";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "service_check",
            "name": "Redis connection",
            "status": 2,
        });
        assert_eq!(parsed, expected);
        let encoded = encode_service_check(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn dogstatsd_complete_service_check() {
        let data = b"_sc|Redis connection|2|d:1663016695|h:test.example.com|#env:dev|m:Redis connection timed out after 10s|c:123abc";
        let parsed = decode(data, 0).expect("failed to decode");
        let expected = literal!({
            "dogstatsd_type": "service_check",
            "name": "Redis connection",
            "status": 2,
            "timestamp": 1663016695 as u32,
            "hostname":"test.example.com",
            "tags": ["env:dev"],
            "message": "Redis connection timed out after 10s",
            "container_id": "123abc",
        });
        assert_eq!(parsed, expected);
        let encoded = encode_service_check(&parsed).expect("failed to encode");
        assert_eq!(encoded, data);
    }

    #[test]
    fn bench() {
        let data = b"foo:1620649445.3351967|h";
        let m = decode(data, 0).expect("failed to decode");
        assert_eq!(&data[..], encode_metric(&m).expect("failed to encode"));

        let data = b"foo1:12345|c";
        let m = decode(data, 0).expect("failed to decode");
        assert_eq!(&data[..], encode_metric(&m).expect("failed to encode"));

        let data = b"foo2:1234567890|c";
        let m = decode(data, 0).expect("failed to decode");
        assert_eq!(&data[..], encode_metric(&m).expect("failed to encode"));

        let data = b"_sc|Redis connection|2|d:1663016695|h:test.example.com|#env:dev|m:Redis connection timed out after 10s|c:123abc";
        let m = decode(data, 0).expect("failed to decode");
        assert_eq!(&data[..], encode_service_check(&m).expect("failed to encode"));

        let data = b"_sc|Redis connection|2";
        let m = decode(data, 0).expect("failed to decode");
        assert_eq!(&data[..], encode_service_check(&m).expect("failed to encode"));

        let data = b"_e{21,36}:An exception occurred|Cannot parse CSV file from 10.0.0.17";
        let m = decode(data, 0).expect("failed to decode");
        assert_eq!(&data[..], encode_event(&m).expect("failed to encode"));

        let data = b"_e{21,36}:An exception occurred|Cannot parse CSV file from 10.0.0.17|#env:dev,test:testing";
        let m = decode(data, 0).expect("failed to decode");
        assert_eq!(&data[..], encode_event(&m).expect("failed to encode"));
    }
}
