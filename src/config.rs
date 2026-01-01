use std::fs;
use std::error::Error;

#[derive(Debug, Clone)]
pub struct Config {
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub server_name: String,        // NEW: For virtual hosting
    pub host: String,
    pub ports: Vec<u16>,
    pub default_server: bool,       // NEW: Mark as default for this (host, port)
    pub error_pages: Vec<ErrorPage>,
    pub client_max_body_size: usize,
    pub root: String,       // NEW: Server-level root directory
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone)]
pub struct ErrorPage {
    pub code: u16,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct Route {
    pub path: String,
    pub methods: Vec<String>,
    pub root: String,
    pub default_file: Option<String>,
    pub redirect: Option<String>,   // NEW: HTTP redirect
    pub cgi: Option<String>,        // NEW: CGI extension (e.g., ".py", ".php")
    pub list_directory: Option<bool>, // NEW: Enable/disable directory listing
}

fn indent_level(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

fn parse_ports(lines: &[String], start: usize) -> Result<(Vec<u16>, usize), Box<dyn Error>> {
    let mut ports = Vec::new();
    let mut i = start;

    if indent_level(&lines[i]) != 4 || lines[i].trim() != "ports:" {
        return Err("Expected 'ports:'".into());
    }
    i += 1;

    while i < lines.len() {
        let lvl = indent_level(&lines[i]);
        let line = lines[i].trim();
        if lvl == 6 && line.starts_with("-") {
            let port = line[1..].trim().parse::<u16>()?;
            ports.push(port);
            i += 1;
        } else {
            break;
        }
    }

    if ports.is_empty() {
        return Err("ports must contain at least one value".into());
    }

    Ok((ports, i))
}

fn parse_error_pages(lines: &[String], start: usize) -> Result<(Vec<ErrorPage>, usize), Box<dyn Error>> {
    let mut pages = Vec::new();
    let mut i = start;

    if indent_level(&lines[i]) != 4 || lines[i].trim() != "error_pages:" {
        return Err("Expected 'error_pages:'".into());
    }
    i += 1;

    // Support multiple error pages
    while i < lines.len() {
        let lvl = indent_level(&lines[i]);
        if lvl != 6 {
            break;
        }

        let line = lines[i].trim();
        if let Some((code, path)) = line.split_once(':') {
            pages.push(ErrorPage {
                code: code.trim().parse::<u16>()?,
                path: path.trim().trim_matches('"').to_string(),
            });
            i += 1;
        } else {
            break;
        }
    }

    Ok((pages, i))
}

fn parse_route(lines: &[String], start: usize) -> Result<(Route, usize), Box<dyn Error>> {
    let mut route = Route {
        path: String::new(),
        methods: Vec::new(),
        root: "".to_string(),
        default_file: None,
        redirect: None,
        cgi: None,
        list_directory: None,
    };

    let mut i = start;

    if indent_level(&lines[i]) != 6 || !lines[i].trim().starts_with("-") {
        return Err("Expected route entry".into());
    }

    // Parse first line (may contain inline key-value)
    let first_line = lines[i].trim()[1..].trim();
    if !first_line.is_empty() {
        if let Some((key, value)) = first_line.split_once(':') {
            parse_route_field(&mut route, key, value)?;
        }
    }
    i += 1;

    // Parse subsequent indented fields
    while i < lines.len() && indent_level(&lines[i]) == 8 {
        let line = lines[i].trim();
        if let Some((key, value)) = line.split_once(':') {
            parse_route_field(&mut route, key, value)?;
        }
        i += 1;
    }

    // Validation: path and methods are required
    if route.path.is_empty() {
        return Err("Route missing 'path'".into());
    }
    if route.methods.is_empty() {
        return Err("Route missing 'methods'".into());
    }
    if route.root.is_empty() {
        return Err("Route missing 'root'".into());
    }

    Ok((route, i))
}

fn parse_route_field(route: &mut Route, key: &str, value: &str) -> Result<(), Box<dyn Error>> {
    match key.trim() {
        "path" => route.path = value.trim().trim_matches('"').to_string(),
        "methods" => {
            let mut v = value.trim();
            if v.starts_with('[') && v.ends_with(']') {
                v = &v[1..v.len()-1];
            }
            route.methods = v.split(',')
                .map(|s| s.trim().trim_matches('"').to_uppercase())
                .filter(|s| !s.is_empty())
                .collect();
        }
        "root" => route.root = value.trim().trim_matches('"').to_string(),
        "default_file" => route.default_file = Some(value.trim().trim_matches('"').to_string()),
        "redirect" => route.redirect = Some(value.trim().trim_matches('"').to_string()),
        "cgi" => route.cgi = Some(value.trim().trim_matches('"').to_string()),
        "list_directory" => {
            let val = value.trim().to_lowercase();
            route.list_directory = Some(val == "true" || val == "yes" || val == "1");
        }
        _ => return Err(format!("Unknown route field: {}", key).into()),
    }
    Ok(())
}

fn parse_server(lines: &[String], start: usize) -> Result<(ServerConfig, usize), Box<dyn Error>> {
    let mut server_name = None;
    let mut host = None;
    let mut default_server = false;
    let mut client_max_body_size = None;
    let mut root = String::new();
    let mut ports = Vec::new();
    let mut error_pages = Vec::new();
    let mut routes = Vec::new();

    let mut i = start;

    // Parse first line: "- host: ..." or "- server_name: ..."
    if indent_level(&lines[i]) == 2 && lines[i].trim().starts_with("-") {
        let inner = lines[i].trim()[1..].trim();
        if let Some((key, value)) = inner.split_once(':') {
            match key.trim() {
                "host" => host = Some(value.trim().to_string()),
                "server_name" => server_name = Some(value.trim().trim_matches('"').to_string()),
                _ => return Err(format!("Expected 'host' or 'server_name' after '-', got '{}'", key).into()),
            }
        } else {
            return Err("Expected 'host:' or 'server_name:' after '-'".into());
        }
        i += 1;
    } else {
        return Err("Expected '- host:' or '- server_name:' for server entry".into());
    }

    // Parse remaining server fields
    while i < lines.len() {
        let lvl = indent_level(&lines[i]);
        let line = lines[i].trim();

        match line {
            _ if lvl == 4 && line.starts_with("server_name:") => {
                server_name = Some(line[12..].trim().trim_matches('"').to_string());
                i += 1;
            }
            _ if lvl == 4 && line.starts_with("host:") => {
                host = Some(line[5..].trim().to_string());
                i += 1;
            }
            _ if lvl == 4 && line == "ports:" => {
                let (p, ni) = parse_ports(lines, i)?;
                ports = p;
                i = ni;
            }
            _ if lvl == 4 && line.starts_with("default_server:") => {
                let val = line[15..].trim().to_lowercase();
                default_server = val == "true" || val == "yes" || val == "1";
                i += 1;
            }
            _ if lvl == 4 && line == "error_pages:" => {
                let (ep, ni) = parse_error_pages(lines, i)?;
                error_pages = ep;
                i = ni;
            }
            _ if lvl == 4 && line.starts_with("client_max_body_size:") => {
                client_max_body_size = Some(line[21..].trim().parse::<usize>()?);
                i += 1;
            }
            _ if lvl == 4 && line.starts_with("root:") => {
                root = line[5..].trim().trim_matches('"').to_string();
                i += 1;
            }
            _ if lvl == 4 && line == "routes:" => {
                i += 1;
                while i < lines.len() && indent_level(&lines[i]) == 6 && lines[i].trim().starts_with("-") {
                    let (r, ni) = parse_route(lines, i)?;
                    routes.push(r);
                    i = ni;
                }
            }
            _ if lvl == 2 && line.starts_with("-") => {
                break; // Next server
            }
            _ if lvl < 4 => {
                break; // End of this server block
            }
            _ => return Err(format!("Unknown server field or invalid indentation: {}", line).into()),
        }
    }

    // Build server config with defaults
    Ok((
        ServerConfig {
            server_name: server_name.unwrap_or_else(|| host.clone().unwrap_or_else(|| "_".to_string())),
            host: host.ok_or("Missing 'host'")?,
            ports: if ports.is_empty() { vec![80] } else { ports },
            default_server,
            error_pages,
            client_max_body_size: client_max_body_size.unwrap_or(1_000_000), // 1MB default
            root,
            routes,
        },
        i,
    ))
}

pub fn load_config(path: &str) -> Result<Config, Box<dyn Error>> {
    let content = fs::read_to_string(path)?;

    let mut lines = Vec::new();
    for raw in content.lines() {
        let clean = raw.split('#').next().unwrap();
        if !clean.trim().is_empty() {
            lines.push(clean.to_string());
        }
    }

    if lines.is_empty() {
        return Err("Empty config file".into());
    }

    if indent_level(&lines[0]) != 0 || lines[0].trim() != "servers:" {
        return Err("Config must start with 'servers:'".into());
    }

    let mut servers = Vec::new();
    let mut i = 1;

    while i < lines.len() {
        if indent_level(&lines[i]) == 2 && lines[i].trim().starts_with("-") {
            let (server, ni) = parse_server(&lines, i)?;
            servers.push(server);
            i = ni;
        } else {
            return Err(format!("Expected server list item at line {}: {}", i + 1, lines[i]).into());
        }
    }

    if servers.is_empty() {
        return Err("Config must contain at least one server".into());
    }

    Ok(Config { servers })
}