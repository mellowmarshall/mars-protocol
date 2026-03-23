use mesh_core::identity::Keypair;
use mesh_core::hash::schema_hash;
use mesh_core::routing::routing_key;

fn main() {
    // Test vector keypair (same as Appendix C)
    let secret = [0x01u8; 32];
    let kp = Keypair::from_bytes(&secret);
    let identity = kp.identity();
    
    println!("=== Test Vector Values ===");
    println!("DID: {}", identity.did());
    println!("Node ID: {}", identity.node_id());
    println!();
    
    // Schema hashes
    let sh = schema_hash("core/capability");
    println!("schema_hash(core/capability): {sh}");
    let sh2 = schema_hash("core/schema");
    println!("schema_hash(core/schema): {sh2}");
    println!();
    
    // Routing keys
    let rk = routing_key("compute/inference/text-generation");
    println!("routing_key(compute/inference/text-generation): {rk}");
    let rk2 = routing_key("compute");
    println!("routing_key(compute): {rk2}");
}
