//! Build the libp2p transport stack used by every node.

use libp2p::{
    PeerId, Transport,
    core::{muxing::StreamMuxerBox, transport::Boxed, upgrade},
    identity::Keypair,
    noise, quic, tcp, yamux,
};

use crate::error::{Error, Result};

/// Build a boxed transport that multiplexes QUIC + TCP / noise / yamux.
///
/// Returns a transport ready to be plugged into `SwarmBuilder`.
pub fn build_transport(keypair: &Keypair) -> Result<Boxed<(PeerId, StreamMuxerBox)>> {
    let quic_transport = quic::tokio::Transport::new(quic::Config::new(keypair));
    let tcp_transport = tcp::tokio::Transport::new(tcp::Config::new().nodelay(true));
    let noise = noise::Config::new(keypair).map_err(|e| Error::Transport(e.to_string()))?;
    let yamux = yamux::Config::default();
    let tcp_upgraded = tcp_transport
        .upgrade(upgrade::Version::V1Lazy)
        .authenticate(noise)
        .multiplex(yamux);

    Ok(quic_transport
        .map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn)))
        .or_transport(tcp_upgraded.map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer))))
        .map(|either, _| either.into_inner())
        .boxed())
}

/// QUIC-free transport for Compose + CI (spec §4.1 — devnet TCP baseline).
///
/// Stack: DNS resolver → TCP (`nodelay`) → Noise (XX, ephemeral keys) → Yamux.
/// The DNS layer is required so bootstrap multiaddrs of the form
/// `/dns4/<hostname>/tcp/<port>` (Compose service names) can be resolved
/// before dialing. Without it libp2p reports `Multiaddr is not supported`
/// and refuses to dial.
///
/// The existing [`build_transport`] remains for callers that still want
/// QUIC + TCP until QUIC pinning lands.
pub fn build_transport_tcp_only(keypair: &Keypair) -> Result<Boxed<(PeerId, StreamMuxerBox)>> {
    let tcp_transport = tcp::tokio::Transport::new(tcp::Config::new().nodelay(true));
    let dns_transport = libp2p::dns::tokio::Transport::system(tcp_transport)
        .map_err(|e| Error::Transport(format!("dns resolver: {e}")))?;
    let noise = noise::Config::new(keypair).map_err(|e| Error::Transport(e.to_string()))?;
    let yamux = yamux::Config::default();
    Ok(dns_transport
        .upgrade(upgrade::Version::V1Lazy)
        .authenticate(noise)
        .multiplex(yamux)
        .map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer)))
        .boxed())
}
