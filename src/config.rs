use std::fs;
use std::error::Error;

#[derive(Debug)]
pub struct Config {
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug)]
pub struct ServerConfig {
    pub host: String,
    pub ports: Vec<u16>,
    pub error_pages: Vec<ErrorPage>,
    pub client_max_body_size: usize,
    pub routes: Vec<Route>,
}

#[derive(Debug)]
pub struct ErrorPage {
    pub code: u16,
    pub path: String,
}

#[derive(Debug)]
pub struct Route {
    pub path: String,
    pub methods: Vec<String>,
    pub root: Option<String>,
    pub default_file: Option<String>,
    pub redirect: Option<String>,
    pub cgi: Option<String>,
    pub list_directory: Option<bool>,
}

// ===================================
// Utilitaire pour calculer indentation
// ===================================
fn indent_level(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

// ===================================
// Parser des ports
// ===================================
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

// ===================================
// Parser des error_pages
// ===================================
fn parse_error_pages(lines: &[String], start: usize) -> Result<(Vec<ErrorPage>, usize), Box<dyn Error>> {
    let mut pages = Vec::new();
    let mut i = start;

    if indent_level(&lines[i]) != 4 || lines[i].trim() != "error_pages:" {
        return Err("Expected 'error_pages:'".into());
    }
    i += 1;

    let lvl = indent_level(&lines[i]);
    if lvl != 6 {
        return Err("Invalid error_pages indentation".into());
    }

    let (code, path) = lines[i].trim()
        .split_once(':')
        .ok_or("Invalid error_pages format")?;

    pages.push(ErrorPage {
        code: code.trim().parse::<u16>()?,
        path: path.trim().trim_matches('"').to_string(),
    });

    Ok((pages, i + 1))
}

// ===================================
// Parser d'une route
// ===================================
fn parse_route(lines: &[String], start: usize) -> Result<(Route, usize), Box<dyn Error>> {
    let mut route = Route {
        path: String::new(),
        methods: Vec::new(),
        root: None,
        default_file: None,
        redirect: None,
        cgi: None,
        list_directory: None,
    };

    let mut i = start;

    if indent_level(&lines[i]) != 6 || !lines[i].trim().starts_with("-") {
        return Err("Expected route entry".into());
    }

    // La ligne peut contenir directement "path: ..."
    let first_line = lines[i].trim()[1..].trim();
    if !first_line.is_empty() {
        if let Some((key, value)) = first_line.split_once(':') {
            match key {
                "path" => route.path = value.trim().trim_matches('"').to_string(),
                "methods" => {
                    let mut v = value.trim();
                    if v.starts_with('[') && v.ends_with(']') {
                        v = &v[1..v.len()-1];
                    }
                    route.methods = v.split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "root" => route.root = Some(value.trim().trim_matches('"').to_string()),
                "default_file" => route.default_file = Some(value.trim().trim_matches('"').to_string()),
                _ => return Err(format!("Unknown route field: {}", key).into()),
            }
        }
    }
    i += 1;

    // Parser les autres champs en dessous
    while i < lines.len() && indent_level(&lines[i]) == 8 {
        let line = lines[i].trim();
        if let Some((key, value)) = line.split_once(':') {
            match key {
                "path" => route.path = value.trim().trim_matches('"').to_string(),
                "methods" => {
                    let mut v = value.trim();
                    if v.starts_with('[') && v.ends_with(']') {
                        v = &v[1..v.len()-1];
                    }
                    route.methods = v.split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "root" => route.root = Some(value.trim().trim_matches('"').to_string()),
                "default_file" => route.default_file = Some(value.trim().trim_matches('"').to_string()),
                _ => return Err(format!("Unknown route field: {}", key).into()),
            }
        }
        i += 1;
    }

    if route.path.is_empty() || route.methods.is_empty() || route.root.is_none() || route.default_file.is_none() {
        return Err("Incomplete route".into());
    }

    Ok((route, i))
}

// ===================================
// Parser d'un serveur
// ===================================
fn parse_server(lines: &[String], start: usize) -> Result<(ServerConfig, usize), Box<dyn Error>> {
    let mut host = None;
    let mut client_max_body_size = None;
    let mut ports = Vec::new();
    let mut error_pages = Vec::new();
    let mut routes = Vec::new();

    let mut i = start;

    // Gérer ligne "- host: ..."
    if indent_level(&lines[i]) == 2 && lines[i].trim().starts_with("-") {
        let inner = lines[i].trim()[1..].trim();
        if inner.starts_with("host:") {
            host = Some(inner[5..].trim().to_string());
        } else {
            return Err("Expected 'host:' after '-'".into());
        }
        i += 1;
    } else {
        return Err("Expected '- host:' for server entry".into());
    }

    while i < lines.len() {
        let lvl = indent_level(&lines[i]);
        let line = lines[i].trim();

        match line {
            _ if lvl == 4 && line == "ports:" => {
                let (p, ni) = parse_ports(lines, i)?;
                ports = p;
                i = ni;
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
            _ if lvl == 4 && line == "routes:" => {
                i += 1;
                while i < lines.len() && indent_level(&lines[i]) == 6 && lines[i].trim().starts_with("-") {
                    let (r, ni) = parse_route(lines, i)?;
                    routes.push(r);
                    i = ni;
                }
            }
            _ if lvl == 2 && line.starts_with("-") => {
                break; // prochain serveur
            }
            _ => return Err(format!("Unknown server field or invalid indentation: {}", line).into()),
        }
    }

    Ok((
        ServerConfig {
            host: host.ok_or("Missing host")?,
            ports,
            error_pages,
            client_max_body_size: client_max_body_size.ok_or("Missing client_max_body_size")?,
            routes,
        },
        i,
    ))
}

// ===================================
// Fonction principale
// ===================================
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

    Ok(Config { servers })
}

