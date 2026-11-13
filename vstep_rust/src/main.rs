fn main() {
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "5050".to_string())
        .parse::<u16>()
        .unwrap_or(5050);
    vstep_engine::start_server(port);
}
