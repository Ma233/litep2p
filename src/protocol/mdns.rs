// Copyright 2018 Parity Technologies (UK) Ltd.
// Copyright 2023 litep2p developers
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use crate::{error::Error, transport::TransportContext};

use multiaddr::Multiaddr;
use simple_dns::{
    rdata::{RData, PTR, TXT},
    Name, Packet, PacketFlag, ResourceRecord, CLASS,
};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

/// Logging target for the file.
const LOG_TARGET: &str = "mdns";

/// IPv4 multicast address.
const IPV4_MULTICAST_ADDRESS: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);

/// IPV4 multicast port.
const IPV4_MULTICAST_PORT: u16 = 5353;

/// Service name.
const SERVICE_NAME: &str = "_p2p._udp.local";

/// mDNS configuration.
#[derive(Debug)]
pub struct Config {
    /// How often the network should be queried for new peers.
    query_interval: Duration,
}

/// Main mDNS object.
pub struct Mdns {
    /// UDP socket for multicast requests/responses.
    socket: UdpSocket,

    /// mDNS configuration.
    config: Config,

    /// Transport context.
    context: TransportContext,

    /// Buffer for incoming messages.
    receive_buffer: Vec<u8>,

    /// Listen addresses.
    listen_addresses: HashSet<Multiaddr>,
}

impl Mdns {
    /// Create new [`Mdns`].
    pub fn new(
        config: Config,
        context: TransportContext,
        listen_addresses: Vec<Multiaddr>,
    ) -> crate::Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
        socket.bind(
            &SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), IPV4_MULTICAST_PORT).into(),
        )?;
        socket.set_multicast_loop_v4(true)?;
        socket.set_multicast_ttl_v4(255)?;
        socket.join_multicast_v4(&IPV4_MULTICAST_ADDRESS, &Ipv4Addr::UNSPECIFIED)?;

        Ok(Self {
            config,
            context,
            receive_buffer: vec![0u8; 4096],
            socket: UdpSocket::from_std(std::net::UdpSocket::from(socket))?,
            listen_addresses: HashSet::from_iter(listen_addresses.into_iter()),
        })
    }

    /// Send mDNS query on the network.
    async fn on_outbound_request(&mut self) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, "send mdns query");

        Ok(())
    }

    /// Handle inbound query.
    fn on_inbound_request(&self, packet: Packet) -> Option<Vec<u8>> {
        tracing::debug!(target: LOG_TARGET, ?packet, "handle inbound request");

        let mut packet = Packet::new_reply(packet.id());
        let srv_name = Name::new_unchecked(SERVICE_NAME);

        packet.answers.push(ResourceRecord::new(
            srv_name.clone(),
            CLASS::IN,
            360,
            RData::PTR(PTR(Name::new_unchecked(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ))),
        ));

        // TODO: use correct addresses
        let mut record = TXT::new();
        record
            .add_string(
                "dnsaddr=/ip6/::1/tcp/8888/p2p/12D3KooWNP463TyS3vUpmekjjZ2dg7xy1WHNMM7MqfsMevMTgzew",
            )
            .expect("valid string");

        packet.additional_records.push(ResourceRecord {
            name: Name::new_unchecked("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            class: CLASS::IN,
            ttl: 360,
            rdata: RData::TXT(record),
            cache_flush: false,
        });

        Some(packet.build_bytes_vec().expect("valid packet"))
    }

    fn on_inbound_response(&self, packet: Packet) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?packet, "handle inbound response");

        Ok(())
    }

    /// Event loop for [`Mdns`].
    pub(crate) async fn start(mut self) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, "starting mdns event loop");

        loop {
            tokio::select! {
                result = self.socket.recv_from(&mut self.receive_buffer) => match result {
                    Ok((nread, address)) => match Packet::parse(&self.receive_buffer[..nread]) {
                        Ok(packet) => match packet.has_flags(PacketFlag::RESPONSE) {
                            true => {
                                tracing::error!(target: LOG_TARGET, ?address, "mdns response received");

                                let _ = self.on_inbound_response(packet);
                            }
                            false => if let Some(response) = self.on_inbound_request(packet) {
                                self.socket
                                    .send_to(&response, (IPV4_MULTICAST_ADDRESS, IPV4_MULTICAST_PORT))
                                    .await?;
                            }
                        }
                        Err(error) => tracing::debug!(
                            target: LOG_TARGET,
                            ?address,
                            ?error,
                            ?nread,
                            "failed to parse mdns packet"
                        ),
                    }
                    Err(error) => {
                        tracing::error!(target: LOG_TARGET, ?error, "failed to read from socket");
                        return Err(Error::from(error));
                    }
                },
                _ = tokio::time::sleep(self.config.query_interval) => {
                    if let Err(error) = self.on_outbound_request().await {
                        tracing::error!(target: LOG_TARGET, ?error, "failed to send mdns query");
                        return Err(error);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::ed25519::Keypair;
    use tokio::sync::mpsc::channel;

    #[tokio::test]
    async fn mdns_works() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (tx, _rx) = channel(64);

        let mdns = Mdns::new(
            Config {
                query_interval: Duration::from_secs(60),
            },
            TransportContext::new(Keypair::generate(), tx),
            Vec::new(),
        )
        .unwrap();

        mdns.start().await.unwrap();
    }
}