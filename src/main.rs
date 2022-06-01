use libp2p::{identity, PeerId, noise, mplex::MplexConfig, 
floodsub::{Topic, Floodsub, FloodsubEvent}, NetworkBehaviour, 
swarm::{behaviour, NetworkBehaviourEventProcess, SwarmBuilder, SwarmEvent}, 
mdns::{MdnsEvent, Mdns}, Multiaddr, core::transport::upgrade, Transport};
use tokio::io::{self, AsyncBufReadExt};
use libp2p::futures::StreamExt;

use libp2p::tcp;

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {

    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());
    println!("local peer id is {:?}", peer_id);

    let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
        .into_authentic(&id_keys)
        .expect("Signing lipp2p noise Keypair failed");

    let transport = tcp::TokioTcpConfig::new()
        .nodelay(true)
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::NoiseConfig::xx(noise_keys).into_authenticated())
        .multiplex(MplexConfig::new())
        .boxed();

    let floodsub_topic = Topic::new("chat");

    #[derive(NetworkBehaviour)]
    #[behaviour(event_process = true)]
    struct MyBehaviour{
        floodsub: Floodsub,
        mdns: Mdns,
    }

    impl NetworkBehaviourEventProcess<FloodsubEvent> for MyBehaviour{
        //called when floodsub produces an event
        fn inject_event(&mut self, message: FloodsubEvent){
            if let FloodsubEvent::Message(message) = message{
                println!(
                    "Received: '{:?}' from {:?}",
                    String::from_utf8_lossy(&message.data),
                    message.source
                );
            }
        }
    }

     impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour{
        //called when floodsub produces an event
        fn inject_event(&mut self, event: MdnsEvent){
            match event{
                MdnsEvent::Discovered(list) => {
                    for (peer, _) in list {
                        self.floodsub.add_node_to_partial_view(peer);
                    }
                }
                MdnsEvent::Expired(list) => {
                    for (peer, _) in list {
                        if !self.mdns.has_node(&peer) {
                            self.floodsub.remove_node_from_partial_view(&peer);
                        }
                    }
                }
            }
        }
    }

    let mut swarm = {
        let mdns = Mdns::new(Default::default()).await?;
        let mut behaviour = MyBehaviour {
            floodsub: Floodsub::new(peer_id.clone()),
            mdns,
        };

    
        behaviour.floodsub.subscribe(floodsub_topic.clone());

        SwarmBuilder::new(transport, behaviour, peer_id)
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        }))
        .build()
    };

    if let Some(to_dial) = std::env::args().nth(1) {
        let addr: Multiaddr = to_dial.parse()?;
        swarm.dial(addr)?;
    }

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    loop{
        tokio::select! {
            line = stdin.next_line() => {
                let line = line?.expect("stdin closed");
                swarm.behaviour_mut().floodsub.publish(floodsub_topic.clone(), line.as_bytes());
            }
            event = swarm.select_next_some() => {
                if let SwarmEvent::NewListenAddr { address, .. } = event {
                    println!("listening on {:?}", address);
                }
            }
        }
    }

}
