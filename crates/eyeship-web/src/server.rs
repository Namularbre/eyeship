//! The only impure part of the interface: the HTTP loop (plan §6).

use std::io;
use std::net::{IpAddr, SocketAddr};

use tiny_http::{Header, Method, Server};

use crate::handler::{DeploymentSource, Request, handle};

/// A bound, not yet serving, HTTP server.
pub struct Bound {
    server: Server,
    address: SocketAddr,
}

impl Bound {
    /// The address actually bound (useful with port 0).
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Whether the server only listens on this machine.
    pub fn is_local(&self) -> bool {
        self.address.ip().is_loopback()
    }

    /// Answers requests one after the other, forever.
    pub fn serve(self, source: &dyn DeploymentSource) {
        let local_only = self.is_local();
        for request in self.server.incoming_requests() {
            let host = request
                .headers()
                .iter()
                .find(|header| header.field.equiv("Host"))
                .map(|header| header.value.as_str().to_owned());
            let method = request.method().as_str().to_owned();
            let response = handle(
                &Request {
                    method: &method,
                    url: request.url(),
                    host: host.as_deref(),
                },
                source,
                local_only,
            );
            let body = if *request.method() == Method::Head {
                Vec::new()
            } else {
                response.body.into_bytes()
            };
            let headers = response.headers.iter().filter_map(|(name, value)| {
                Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
            });
            let reply = headers.fold(
                tiny_http::Response::from_data(body).with_status_code(response.status),
                tiny_http::Response::with_header,
            );
            // A client that went away is not an error of the server.
            let _ = request.respond(reply);
        }
    }
}

/// Binds `host:port`.
pub fn bind(host: IpAddr, port: u16) -> io::Result<Bound> {
    let server = Server::http(SocketAddr::new(host, port)).map_err(io::Error::other)?;
    let address = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| io::Error::other("not an IP address"))?;
    Ok(Bound { server, address })
}
