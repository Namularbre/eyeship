//! Normalization of `ports`, short and long syntax.

use std::net::IpAddr;

use eyeship_domain::{Location, PortMapping, PortRange, Protocol};
use serde_saphyr::Spanned;

use crate::context::Context;
use crate::raw::{RawPort, Scalar};

/// Maps every entry of `ports`; entries that cannot be parsed are skipped.
pub(crate) fn map_ports(ctx: &Context<'_>, entries: &[Spanned<RawPort>]) -> Vec<PortMapping> {
    entries
        .iter()
        .filter_map(|entry| map_port(ctx, &entry.value, ctx.location(&entry.referenced)))
        .collect()
}

fn map_port(ctx: &Context<'_>, port: &RawPort, location: Location) -> Option<PortMapping> {
    let text = |scalar: &Scalar| ctx.interpolate(scalar.as_str());
    match port {
        RawPort::Short(spec) => parse_short(&text(spec), location),
        RawPort::Long {
            target,
            published,
            host_ip,
            protocol,
        } => Some(PortMapping {
            host_ip: host_ip.as_ref().map(text).filter(|ip| !ip.is_empty()),
            published: parse_published(published.as_ref().map(text).as_deref())?,
            target: parse_range(&text(target.as_ref()?))?,
            protocol: protocol
                .as_ref()
                .map_or(Some(Protocol::Tcp), |p| parse_protocol(&text(p)))?,
            location,
        }),
        RawPort::Other(_) => None,
    }
}

/// Parses `[host_ip:][published:]target[/protocol]`, with `[ipv6]` hosts.
fn parse_short(spec: &str, location: Location) -> Option<PortMapping> {
    let spec = spec.trim();
    let (addresses, protocol) = match spec.rsplit_once('/') {
        Some((addresses, protocol)) => (addresses, parse_protocol(protocol)?),
        None => (spec, Protocol::Tcp),
    };
    let parts: Vec<&str> = addresses.rsplitn(3, ':').collect();
    let (host_ip, published, target) = match parts.as_slice() {
        [target] => (None, None, *target),
        [target, published] => (None, Some(*published), *target),
        [target, published, host_ip] => (Some(*host_ip), Some(*published), *target),
        _ => return None,
    };
    let host_ip = host_ip.map(strip_brackets).filter(|ip| !ip.is_empty());
    if host_ip
        .as_deref()
        .is_some_and(|ip| ip.parse::<IpAddr>().is_err())
    {
        return None;
    }
    Some(PortMapping {
        host_ip,
        published: parse_published(published)?,
        target: parse_range(target)?,
        protocol,
        location,
    })
}

/// `None` or empty means "chosen by Docker"; anything else must be a range.
fn parse_published(published: Option<&str>) -> Option<Option<PortRange>> {
    match published.map(str::trim) {
        None | Some("") => Some(None),
        Some(range) => parse_range(range).map(Some),
    }
}

fn strip_brackets(ip: &str) -> String {
    ip.strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(ip)
        .to_owned()
}

/// Parses `8080` or `8000-8010`.
fn parse_range(text: &str) -> Option<PortRange> {
    let port = |s: &str| s.trim().parse::<u16>().ok();
    match text.split_once('-') {
        Some((start, end)) => {
            let (start, end) = (port(start)?, port(end)?);
            (start <= end).then_some(PortRange { start, end })
        }
        None => port(text).map(PortRange::single),
    }
}

fn parse_protocol(text: &str) -> Option<Protocol> {
    match text.trim().to_ascii_lowercase().as_str() {
        "tcp" => Some(Protocol::Tcp),
        "udp" => Some(Protocol::Udp),
        "sctp" => Some(Protocol::Sctp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::test_support::*;

    fn ports(yaml: &str) -> Vec<PortMapping> {
        with_context(&[], &[("PORT", "9000")], |ctx| {
            map_ports(ctx, &service(yaml).ports.unwrap_or_default())
        })
    }

    fn single(yaml: &str) -> (Option<String>, Option<PortRange>, PortRange, Protocol) {
        let mapped = ports(&format!("ports: [{yaml}]"));
        assert_eq!(mapped.len(), 1, "{yaml}");
        let port = &mapped[0];
        (
            port.host_ip.clone(),
            port.published,
            port.target,
            port.protocol,
        )
    }

    fn range(start: u16, end: u16) -> PortRange {
        PortRange { start, end }
    }

    fn one(port: u16) -> PortRange {
        PortRange::single(port)
    }

    #[test]
    fn target_only() {
        assert_eq!(single("\"80\""), (None, None, one(80), Protocol::Tcp));
        assert_eq!(single("80"), (None, None, one(80), Protocol::Tcp));
    }

    #[test]
    fn published_and_target() {
        assert_eq!(
            single("\"8080:80\""),
            (None, Some(one(8080)), one(80), Protocol::Tcp)
        );
    }

    #[test]
    fn host_ip() {
        assert_eq!(
            single("\"127.0.0.1:8080:80\""),
            (
                Some("127.0.0.1".into()),
                Some(one(8080)),
                one(80),
                Protocol::Tcp
            )
        );
        assert_eq!(
            single("\"[::1]:8080:80\""),
            (Some("::1".into()), Some(one(8080)), one(80), Protocol::Tcp)
        );
    }

    #[test]
    fn host_ip_without_published_port() {
        assert_eq!(
            single("\"127.0.0.1::80\""),
            (Some("127.0.0.1".into()), None, one(80), Protocol::Tcp)
        );
    }

    #[test]
    fn ranges() {
        assert_eq!(
            single("\"8000-8010:8000-8010\""),
            (
                None,
                Some(range(8000, 8010)),
                range(8000, 8010),
                Protocol::Tcp
            )
        );
    }

    #[test]
    fn protocol() {
        assert_eq!(
            single("\"9090:9090/udp\""),
            (None, Some(one(9090)), one(9090), Protocol::Udp)
        );
        assert_eq!(single("\"53/SCTP\"").3, Protocol::Sctp);
    }

    #[test]
    fn interpolated() {
        assert_eq!(single("\"${PORT}:80\"").1, Some(one(9000)));
    }

    #[test]
    fn long_syntax() {
        let yaml = "{target: 80, published: 8080, host_ip: 127.0.0.1, protocol: udp}";
        assert_eq!(
            single(yaml),
            (
                Some("127.0.0.1".into()),
                Some(one(8080)),
                one(80),
                Protocol::Udp
            )
        );
        assert_eq!(
            single("{target: 80, published: \"8000-8001\"}"),
            (None, Some(range(8000, 8001)), one(80), Protocol::Tcp)
        );
        assert_eq!(single("{target: 80}"), (None, None, one(80), Protocol::Tcp));
    }

    #[test]
    fn unparsable_entries_are_skipped() {
        let yaml = "ports: [\"x:80\", \"80/foo\", \"1:2:3:4\", 70000, \"9-1\", [1], {published: 80}, \"81\"]";
        let mapped = ports(yaml);
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].target, one(81));
    }

    #[test]
    fn each_entry_has_its_location() {
        let mapped = ports("ports:\n  - \"80:80\"\n  - target: 81\n");
        let locations: Vec<_> = mapped.iter().map(|p| p.location).collect();
        assert_eq!(locations, [at(2, 5), at(3, 5)]);
    }
}
