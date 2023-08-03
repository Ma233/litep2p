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

use litep2p::{
    config::Litep2pConfigBuilder, crypto::ed25519::Keypair,
    protocol::libp2p::kademlia::ConfigBuilder as KademliaConfigBuilder,
    transport::tcp::config::TransportConfig as TcpTransportConfig, Litep2p,
};

async fn spawn_litep2p(port: u16) {
    let (kad_config1, _kad_handle1) = KademliaConfigBuilder::new().build();
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpTransportConfig {
            listen_address: format!("/ip6/::1/tcp/{port}").parse().unwrap(),
        })
        .with_ipfs_kademlia(kad_config1)
        .build();

    let mut litep2p1 = Litep2p::new(config1).await.unwrap();

    loop {
        let _ = litep2p1.next_event().await;
    }
}

#[tokio::test]
async fn kademlia_supported() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (kad_config1, _kad_handle1) = KademliaConfigBuilder::new().build();
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpTransportConfig {
            listen_address: "/ip6/::1/tcp/8888".parse().unwrap(),
        })
        .with_ipfs_kademlia(kad_config1)
        .build();

    let mut litep2p1 = Litep2p::new(config1).await.unwrap();

    for port in 9000..9003 {
        tokio::spawn(spawn_litep2p(port));
    }

    loop {
        tokio::select! {
            event = litep2p1.next_event() => {
                tracing::info!("litep2p event received: {event:?}");
            }
            // event = kad_handle1.next() => {
            //     tracing::info!("kademlia event received: {event:?}");
            // }
        }
    }
}