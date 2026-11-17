//! sip-uri's own public types stay usable across threads and in async
//! code; the re-exported crates assert theirs.

use sip_uri::{
    HeaderMask, HostMask, OpaqueMask, Parsed, Redacted, Redaction, SipUri, TelUri, Uri, UrnUri,
    UserMask,
};

fn is_send_sync_unpin<T: Send + Sync + Unpin>() {}

#[test]
fn public_types_are_send_sync_unpin() {
    is_send_sync_unpin::<Redaction>();
    is_send_sync_unpin::<Redacted<'static, Uri>>();
    is_send_sync_unpin::<Redacted<'static, SipUri>>();
    is_send_sync_unpin::<Redacted<'static, TelUri>>();
    is_send_sync_unpin::<Redacted<'static, UrnUri>>();
    is_send_sync_unpin::<Parsed<Uri>>();
    is_send_sync_unpin::<Parsed<SipUri>>();
    is_send_sync_unpin::<UserMask>();
    is_send_sync_unpin::<HeaderMask>();
    is_send_sync_unpin::<OpaqueMask>();
    is_send_sync_unpin::<HostMask>();
}
