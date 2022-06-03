pub mod dialer;
pub mod listener;
pub mod protocol;

// TODO: Or should this be `/ipfs/id/1.0.0` for full compliance? I don't see much point in doing
// this. If we speak this protocol we might just get spammed
pub const PROTOCOL: &str = "/itchysats/id/1.0.0";
