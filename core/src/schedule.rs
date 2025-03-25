use serde::{Deserialize, Serialize};

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Healthcheck {
//     test: Vec<String>,
//     interval: String,
//     timeout: String,
//     retries: u32,
//     start_period: String,
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct HostFeatures {
    #[serde(default)]
    daemon_socket: bool,

    #[serde(default)]
    boot_partition: bool,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortSpec {
    pub host_ip: Option<String>,
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VolumeHostPath {
    Named(String),
    Anonymous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSpec {
    pub host_path: VolumeHostPath,
    pub container_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMode {
    // Bridge(String),
    // Container(String),
    Host,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    name: String,
    image: String,

    #[serde(default)]
    environment: Vec<String>,

    #[serde(default)]
    command: String,

    #[serde(default)]
    restart: String,

    #[serde(default)]
    networks: Vec<String>,

    #[serde(default)]
    depends_on: Vec<String>,

    #[serde(default)]
    privileged: bool,

    #[serde(default, deserialize_with = "deserialize_port_specs")]
    ports: Vec<PortSpec>,

    #[serde(default, deserialize_with = "deserialize_volume_specs")]
    volumes: Vec<VolumeSpec>,

    // #[serde(default)]
    // healthcheck: Healthcheck,

    #[serde(default)]
    host_features: HostFeatures,
}

fn default_protocol() -> String {
    "tcp".to_string()
}

impl std::str::FromStr for VolumeSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();

        match parts.len() {
            // Format: container_path
            1 => Ok(VolumeSpec {
                host_path: VolumeHostPath::Anonymous,
                container_path: parts[0].to_string(),
            }),
            // Format: host_path:container_path
            2 => Ok(VolumeSpec {
                host_path: VolumeHostPath::Named(parts[0].to_string()),
                container_path: parts[1].to_string(),
            }),
            _ => Err(format!("Invalid volume specification: {}", s)),
        }
    }
}

impl std::str::FromStr for PortSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Check if protocol is specified
        let (port_part, protocol) = if let Some((port, proto)) = s.split_once('/') {
            (port, proto.to_string())
        } else {
            (s, default_protocol())
        };

        // Parse the port components
        let parts: Vec<&str> = port_part.split(':').collect();

        match parts.len() {
            // Format: container_port
            1 => {
                let port = parts[0]
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid port number: {}", parts[0]))?;

                Ok(PortSpec {
                    host_ip: None,
                    host_port: port,
                    container_port: port,
                    protocol,
                })
            }
            // Format: host_port:container_port
            2 => {
                let host_port = parts[0]
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid host port: {}", parts[0]))?;
                let container_port = parts[1]
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid container port: {}", parts[1]))?;

                Ok(PortSpec {
                    host_ip: None,
                    host_port,
                    container_port,
                    protocol,
                })
            }
            // Format: host_ip:host_port:container_port
            3 => {
                let host_ip = Some(parts[0].to_string());
                let host_port = parts[1]
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid host port: {}", parts[1]))?;
                let container_port = parts[2]
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid container port: {}", parts[2]))?;

                Ok(PortSpec {
                    host_ip,
                    host_port,
                    container_port,
                    protocol,
                })
            }
            _ => Err(format!("Invalid port specification: {}", s)),
        }
    }
}

fn deserialize_port_specs<'de, D>(deserializer: D) -> Result<Vec<PortSpec>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PortMapping {
        AsString(String),
        AsStruct(PortSpec),
    }

    let port_mappings = Vec::<PortMapping>::deserialize(deserializer)?;

    port_mappings
        .into_iter()
        .map(|mapping| match mapping {
            PortMapping::AsString(s) => {
                let mapping_str = s.parse().map_err(serde::de::Error::custom)?;
                Ok(mapping_str)
            }
            PortMapping::AsStruct(spec) => Ok(spec),
        })
        .collect()
}

fn deserialize_volume_specs<'de, D>(deserializer: D) -> Result<Vec<VolumeSpec>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VolumeMapping {
        AsString(String),
        AsStruct(VolumeSpec),
    }

    let volume_mappings = Vec::<VolumeMapping>::deserialize(deserializer)?;

    volume_mappings
        .into_iter()
        .map(|mapping| match mapping {
            VolumeMapping::AsString(s) => {
                let mapping_str = s.parse().map_err(serde::de::Error::custom)?;
                Ok(mapping_str)
            }
            VolumeMapping::AsStruct(spec) => Ok(spec),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    version: String,
    services: Vec<Service>,
}

impl Spec {
    pub fn read_from(path: &str) -> Result<Self, anyhow::Error> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let spec = serde_yaml::from_reader(reader)?;
        Ok(spec)
    }
}

#[cfg(test)]
mod tests {
    use crate::schedule::Spec;


    #[test]
    fn test_example_spec() {
        const EXAMPLE_SPEC: &str = r#"
version: 0.1.0
services:
    -
        name: nginx
        image: nginx:latest
        ports:
            - 80:80
            - 443:443
            - 10.0.0.10:8080:80/tcp
        volumes:
            - /var/www:/var/www
            - /etc/nginx:/etc/nginx
        environment:
            - NGINX_PORT=80
            - NGINX_HOST=localhost
        command: nginx -g 'daemon off;'
        restart: always
        networks:
            - default
            - backend
        depends_on:
            - db
            - cache
        privileged: true
        cap_add:
            - NET_ADMIN
            - SYS_ADMIN
        cap_drop:
            - MKNOD
            - AUDIT_CONTROL
        # healthcheck:
        #     test: ["CMD", "curl", "-f", "http://localhost"]
        #     interval: 30s
        #     timeout: 10s
        #     retries: 3
        #     start_period: 40s
        host_features:
            daemon_socket: true
            boot_partition: true
    "#;

        let spec: Spec = serde_yaml::from_str(EXAMPLE_SPEC).unwrap();
        assert_eq!(spec.version, "0.1.0");
    }


    #[test]
    fn test_simple_spec() {
        const SIMPLE_SPEC: &str = r#"
version: 0.1.0
services:
    -
        name: nginx
        image: nginx:latest
    "#;

        let spec: Spec = serde_yaml::from_str(SIMPLE_SPEC).unwrap();
        assert_eq!(spec.version, "0.1.0");
    }

}
