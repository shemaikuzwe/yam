pub enum StatusCode {
    StatusOk = 200,
    StatusBadRequest = 400,
    StatusInternalServerError = 500,
}
struct Response;
impl Response {
    pub fn send<T>(&self, data: T, status: StatusCode) {}
}
