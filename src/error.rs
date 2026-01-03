use crate::config::ServerConfig;

pub(crate) fn get_error_page_path(server: &ServerConfig, status_code: u16) -> String {
    server
        .error_pages
        .iter()
        .find(|ep| ep.code == status_code)
        .map(|ep| ep.path.clone())
        .unwrap_or_else(|| format!("./error_pages/{}.html", status_code))
}
