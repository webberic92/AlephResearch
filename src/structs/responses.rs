use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Response {
pub status: String,
}