use crate::connectors::impls::gbq::Config;
use crate::connectors::prelude::*;
use async_std::prelude::StreamExt;
use futures::stream;
use googapis::google::cloud::bigquery::storage::v1::append_rows_request::ProtoData;
use googapis::google::cloud::bigquery::storage::v1::big_query_write_client::BigQueryWriteClient;
use googapis::google::cloud::bigquery::storage::v1::table_field_schema::Type as TableType;
use googapis::google::cloud::bigquery::storage::v1::{
    append_rows_request, table_field_schema, write_stream, AppendRowsRequest,
    CreateWriteStreamRequest, ProtoRows, ProtoSchema, TableFieldSchema, WriteStream,
};
use prost::encoding::WireType;
use prost_types::{field_descriptor_proto, DescriptorProto, FieldDescriptorProto};
use std::collections::HashMap;
use tonic::codegen::InterceptedService;
use tonic::metadata::{Ascii, MetadataValue};
use tonic::service::Interceptor;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};
use tonic::{Request, Status};

pub(crate) struct GbqSink {
    client: BigQueryWriteClient<InterceptedService<Channel, AuthInterceptor>>,
    write_stream: WriteStream,
    mapping: JsonToProtobufMapping,
}

pub(crate) struct AuthInterceptor {
    token: MetadataValue<Ascii>,
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> ::std::result::Result<Request<()>, Status> {
        request
            .metadata_mut()
            .insert("authorization", self.token.clone());

        Ok(request)
    }
}

struct Field {
    table_type: TableType,
    tag: u32,

    // ignored if the table_type is not struct
    subfields: HashMap<String, Field>,
}

struct JsonToProtobufMapping {
    fields: HashMap<String, Field>,
    descriptor: DescriptorProto,
}

fn map_field(
    schema_name: &str,
    raw_fields: &Vec<TableFieldSchema>,
) -> (DescriptorProto, HashMap<String, Field>) {
    let mut nested_types = vec![];
    let mut proto_fields = vec![];
    let mut fields = HashMap::new();
    let mut tag = 1;

    for raw_field in raw_fields {
        let mut type_name = None;
        let mut subfields = HashMap::new();

        let table_type =
            if let Some(table_type) = table_field_schema::Type::from_i32(raw_field.r#type) {
                warn!("Found a field of unknown type: {}", raw_field.name);
                table_type
            } else {
                continue;
            };

        let grpc_type = match table_type {
            table_field_schema::Type::Int64 => field_descriptor_proto::Type::Int64,
            TableType::Double => field_descriptor_proto::Type::Double,
            TableType::Bool => field_descriptor_proto::Type::Bool,
            TableType::Bytes => field_descriptor_proto::Type::Bytes,


            TableType::String
            // YYYY-[M]M-[D]D
            | TableType::Date
            // [H]H:[M]M:[S]S[.DDDDDD|.F]
            | TableType::Time
            // YYYY-[M]M-[D]D[( |T)[H]H:[M]M:[S]S[.F]]
            | TableType::Datetime
            // The GEOGRAPHY type is based on the OGC Simple Features specification (SFS)
            | TableType::Geography
            // String, because it's a precise, f32/f64 would lose precision
            | TableType::Numeric
            | TableType::Bignumeric
            // [sign]Y-M [sign]D [sign]H:M:S[.F]
            | TableType::Interval
            | TableType::Json
            // YYYY-[M]M-[D]D[( |T)[H]H:[M]M:[S]S[.F]][time zone]
            | TableType::Timestamp => field_descriptor_proto::Type::String,
            TableType::Struct => {
                let type_name_for_field = format!("struct_{}", raw_field.name);
                let mapped = map_field(&type_name_for_field, &raw_field.fields);
                nested_types.push(mapped.0);
                subfields = mapped.1;

                type_name = Some(type_name_for_field);
                field_descriptor_proto::Type::Message
            }

            TableType::Unspecified => {
                warn!("Found a field of unspecified type: {}", raw_field.name);
                continue;
            }
        };

        proto_fields.push(FieldDescriptorProto {
            name: Some(raw_field.name.to_string()),
            number: Some(tag as i32),
            label: None,
            r#type: Some(i32::from(grpc_type)),
            type_name,
            extendee: None,
            default_value: None,
            oneof_index: None,
            json_name: None,
            options: None,
            proto3_optional: None,
        });

        fields.insert(
            raw_field.name.to_string(),
            Field {
                table_type,
                tag,
                subfields,
            },
        );

        tag += 1;
    }

    (
        DescriptorProto {
            name: Some(schema_name.to_string()),
            field: proto_fields,
            extension: vec![],
            nested_type: nested_types,
            enum_type: vec![],
            extension_range: vec![],
            oneof_decl: vec![],
            options: None,
            reserved_range: vec![],
            reserved_name: vec![],
        },
        fields,
    )
}

fn encode_field(val: &Value, field: &Field, result: &mut Vec<u8>) {
    let tag = field.tag;

    // fixme check which fields are required and fail if they're missing
    // fixme do not panic if the tremor type does not match
    match field.table_type {
        TableType::Double => prost::encoding::double::encode(tag, &val.as_f64().unwrap(), result),
        TableType::Int64 => prost::encoding::int64::encode(tag, &val.as_i64().unwrap(), result),
        TableType::Bool => prost::encoding::bool::encode(tag, &val.as_bool().unwrap(), result),
        TableType::String
        | TableType::Date
        | TableType::Time
        | TableType::Datetime
        | TableType::Timestamp
        | TableType::Numeric
        | TableType::Bignumeric
        | TableType::Geography => {
            prost::encoding::string::encode(tag, &val.as_str().unwrap().to_string(), result);
        }
        TableType::Struct => {
            let mut struct_buf: Vec<u8> = vec![];
            for (k, v) in val.as_object().unwrap() {
                let subfield_description = field.subfields.get(&k.to_string()).unwrap();
                encode_field(v, subfield_description, &mut struct_buf);
            }
            prost::encoding::encode_key(tag, WireType::LengthDelimited, result);
            prost::encoding::encode_varint(struct_buf.len() as u64, result);
            result.append(&mut struct_buf);
        }
        TableType::Bytes => {
            prost::encoding::bytes::encode(tag, &Vec::from(val.as_bytes().unwrap()), result);
        }

        // fixme to test this we need a json field, which we don't have right now
        TableType::Json => {
            prost::encoding::string::encode(tag, &simd_json::to_string(val).unwrap(), result);
        }
        // fixme this is not GA, need to test
        TableType::Interval => {}

        TableType::Unspecified => {
            warn!("Found a field of unspecified type - ignoring.");
        }
    }
}

impl JsonToProtobufMapping {
    pub fn new(vec: &Vec<TableFieldSchema>) -> Self {
        let descriptor = map_field("table", vec);

        Self {
            descriptor: descriptor.0,
            fields: descriptor.1,
        }
    }

    pub fn map(&self, value: &Value) -> Vec<u8> {
        let mut result = vec![];
        if let Some(obj) = value.as_object() {
            for (key, val) in obj {
                if let Some(field) = self.fields.get(&key.to_string()) {
                    encode_field(val, field, &mut result);
                }
            }
        }

        result
    }

    pub fn descriptor(&self) -> &DescriptorProto {
        &self.descriptor
    }
}
impl GbqSink {
    pub async fn new(config: Config) -> Result<Self> {
        let token_metadata_value =
            MetadataValue::from_str(format!("Bearer {}", config.token).as_str())?;

        let tls_config = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(googapis::CERTIFICATES))
            .domain_name("pubsub.googleapis.com");

        let channel = Channel::from_static("https://bigquerystorage.googleapis.com")
            .tls_config(tls_config)?
            .connect()
            .await?;

        let mut client = BigQueryWriteClient::with_interceptor(
            channel,
            AuthInterceptor {
                token: token_metadata_value,
            },
        );

        let write_stream = client
            .create_write_stream(CreateWriteStreamRequest {
                parent: config.table_id.clone(),
                write_stream: Some(WriteStream {
                    name: "".to_string(),
                    r#type: i32::from(write_stream::Type::Committed),
                    create_time: None,
                    commit_time: None,
                    table_schema: None,
                }),
            })
            .await?
            .into_inner();

        let mapping = JsonToProtobufMapping::new(
            &write_stream
                .table_schema
                .as_ref()
                .ok_or(ErrorKind::GbqSinkFailed("Table schema was not provided"))?
                .clone()
                .fields,
        );
        Ok(Self {
            client,
            write_stream,
            mapping,
        })
    }
}

#[async_trait::async_trait]
impl Sink for GbqSink {
    async fn on_event(
        &mut self,
        _input: &str,
        event: Event,
        _ctx: &SinkContext,
        _serializer: &mut EventSerializer,
        _start: u64,
    ) -> Result<SinkReply> {
        let request = AppendRowsRequest {
            write_stream: self.write_stream.name.clone(),
            offset: None,
            trace_id: "".to_string(),
            rows: Some(append_rows_request::Rows::ProtoRows(ProtoData {
                writer_schema: Some(ProtoSchema {
                    proto_descriptor: Some(self.mapping.descriptor().clone()),
                }),
                rows: Some(ProtoRows {
                    serialized_rows: vec![self.mapping.map(event.data.parts().0)],
                }),
            })),
        };

        let mut apnd_response = self
            .client
            .append_rows(stream::iter(vec![request]))
            .await?
            .into_inner();

        if let Some(response) = apnd_response.next().await {
            match response {
                Ok(_) => Ok(SinkReply::ACK),
                Err(e) => {
                    error!("Failed to write event to BigQuery: {}", e);

                    Ok(SinkReply::FAIL)
                }
            }
        } else {
            Ok(SinkReply::FAIL)
        }
    }

    fn auto_ack(&self) -> bool {
        false
    }
}