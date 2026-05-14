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
