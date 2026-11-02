use ela_session::SessionError;
use ironrdp_pdu::mcs::decode_send_data_indication;
use ironrdp_pdu::nego::{
    ConnectionRequest, ConnectionRequestWithOpaqueRoutingToken, OpaqueRoutingToken, RequestFlags,
    SecurityProtocol,
};
use ironrdp_pdu::rdp::server_redirection::{
    ELEMENT_TYPE_CERTIFICATE, ENCODING_TYPE_ASN1_DER, ServerRedirectionFlags, ServerRedirectionPdu,
    decode_server_redirection,
};
use ironrdp_pdu::x224::X224;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

use crate::pinned_certs;

pub(crate) struct Redirect {
    routing_token: Option<String>,
    pub(crate) username: String,
    pub(crate) domain: Option<String>,
    guid: Vec<u8>,
    password: Vec<u8>,
    pub(crate) certificate: Option<String>,
}

impl Redirect {
    pub(crate) fn new(pdu: ServerRedirectionPdu) -> Result<Self, SessionError> {
        // Only the RDSTLS handover is followed: a broker's plain password cookie would need
        // the user's password kept for the life of the Session.
        let not_followable = || {
            SessionError::protocol(
                "the server moved the Session to another host, which Ela can't follow yet",
            )
        };
        if !pdu
            .flags
            .contains(ServerRedirectionFlags::PASSWORD_IS_PK_ENCRYPTED)
            || pdu.target_net_address.is_some()
            || pdu.target_net_addresses.is_some()
        {
            return Err(not_followable());
        }
        let (Some(username), Some(guid), Some(password)) =
            (pdu.username, pdu.redirection_guid, pdu.password)
        else {
            return Err(not_followable());
        };
        let routing_token = pdu
            .load_balance_info
            .map(|info| {
                String::from_utf8(info)
                    .map(|token| token.trim_end_matches("\r\n").to_owned())
                    .map_err(|_| SessionError::protocol("the redirect's routing token isn't text"))
            })
            .transpose()?;
        let certificate = pdu.target_certificate.and_then(|container| {
            container
                .elements
                .into_iter()
                .find(|e| {
                    e.element_type == ELEMENT_TYPE_CERTIFICATE
                        && e.encoding == ENCODING_TYPE_ASN1_DER
                })
                .map(|e| pinned_certs::fingerprint(&e.data))
        });
        Ok(Self {
            routing_token,
            username,
            domain: pdu.domain,
            guid,
            password,
            certificate,
        })
    }

    // ironrdp's connector can't ask for RDSTLS, so this request is sent in its place.
    pub(crate) fn connection_request(&self) -> Result<Vec<u8>, SessionError> {
        let request = ConnectionRequest {
            nego_data: None,
            flags: RequestFlags::empty(),
            protocol: SecurityProtocol::RDSTLS,
            correlation_info: None,
        };
        let encoded = match &self.routing_token {
            Some(token) => {
                ironrdp_core::encode_vec(&X224(ConnectionRequestWithOpaqueRoutingToken {
                    request,
                    routing_token: OpaqueRoutingToken(token.clone()),
                }))
            }
            None => ironrdp_core::encode_vec(&X224(request)),
        };
        encoded.map_err(|e| SessionError::protocol(e.report()))
    }

    pub(crate) async fn authenticate(
        &self,
        stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    ) -> Result<(), SessionError> {
        let io = |e: std::io::Error| SessionError::from_io(&e);
        let mut capabilities = [0; 8];
        stream.read_exact(&mut capabilities).await.map_err(io)?;
        check_capabilities(capabilities)?;
        stream
            .write_all(&self.authentication_request())
            .await
            .map_err(io)?;
        stream.flush().await.map_err(io)?;
        let mut response = [0; 10];
        stream.read_exact(&mut response).await.map_err(io)?;
        check_response(response)
    }

    fn authentication_request(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for word in [
            RDSTLS_VERSION_1,
            TYPE_AUTH_REQUEST,
            DATA_PASSWORD_CREDENTIALS,
        ] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        put_bytes(&mut out, &self.guid);
        put_bytes(&mut out, &utf16_with_nul(&self.username));
        put_bytes(
            &mut out,
            &utf16_with_nul(self.domain.as_deref().unwrap_or("")),
        );
        put_bytes(&mut out, &self.password);
        out
    }
}

const RDSTLS_VERSION_1: u16 = 0x0001;
const TYPE_CAPABILITIES: u16 = 0x0001;
const TYPE_AUTH_REQUEST: u16 = 0x0002;
const TYPE_AUTH_RESPONSE: u16 = 0x0004;
const DATA_CAPABILITIES: u16 = 0x0001;
const DATA_PASSWORD_CREDENTIALS: u16 = 0x0001;
const DATA_RESULT_CODE: u16 = 0x0001;
const RESULT_ACCESS_DENIED: u32 = 0x0000_0005;
const RESULT_LOGON_FAILURE: u32 = 0x0000_052E;

fn words<const N: usize>(bytes: &[u8]) -> [u16; N] {
    std::array::from_fn(|i| u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]))
}

fn check_capabilities(pdu: [u8; 8]) -> Result<(), SessionError> {
    let [version, kind, data, supported] = words(&pdu);
    if version != RDSTLS_VERSION_1
        || kind != TYPE_CAPABILITIES
        || data != DATA_CAPABILITIES
        || supported & RDSTLS_VERSION_1 == 0
    {
        return Err(SessionError::protocol(
            "the redirect target doesn't offer RDSTLS version 1",
        ));
    }
    Ok(())
}

fn check_response(pdu: [u8; 10]) -> Result<(), SessionError> {
    let [version, kind, data] = words(&pdu);
    if version != RDSTLS_VERSION_1 || kind != TYPE_AUTH_RESPONSE || data != DATA_RESULT_CODE {
        return Err(SessionError::protocol(
            "the redirect target sent an unexpected RDSTLS response",
        ));
    }
    match u32::from_le_bytes([pdu[6], pdu[7], pdu[8], pdu[9]]) {
        0 => Ok(()),
        code @ (RESULT_ACCESS_DENIED | RESULT_LOGON_FAILURE) => Err(SessionError::AuthFailed(
            format!("the redirect target refused the handed-over sign-in (RDSTLS {code:#x})"),
        )),
        code => Err(SessionError::protocol(format!(
            "the redirect target refused the handed-over sign-in (RDSTLS {code:#x})"
        ))),
    }
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn utf16_with_nul(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

// FreeRDP-based servers such as GNOME Remote Desktop write PDU version 1 into the
// redirect's Share Control Header where Windows writes 0, and ironrdp refuses anything but 0.
pub(crate) fn freerdp_redirection(
    frame: &[u8],
    io_channel_id: u16,
) -> Option<ServerRedirectionPdu> {
    const PDU_TYPE_WITH_VERSION: u16 = 0x0010 | 0x000A;
    let indication = decode_send_data_indication(frame).ok()?;
    let data = indication.user_data;
    if indication.channel_id != io_channel_id
        || data.get(2..4)? != PDU_TYPE_WITH_VERSION.to_le_bytes()
    {
        return None;
    }
    let mut patched = data.to_vec();
    patched[2..4].copy_from_slice(&0x000Au16.to_le_bytes());
    decode_server_redirection(&patched).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironrdp_pdu::mcs::SendDataIndication;
    use ironrdp_pdu::rdp::headers::{ShareControlHeader, ShareControlPdu};
    use ironrdp_pdu::rdp::server_redirection::TargetCertificateContainer;
    use std::borrow::Cow;

    const IO_CHANNEL: u16 = 1003;

    fn handover() -> ServerRedirectionPdu {
        ServerRedirectionPdu {
            session_id: 0,
            flags: ServerRedirectionFlags::LOAD_BALANCE_INFO
                | ServerRedirectionFlags::USERNAME
                | ServerRedirectionFlags::PASSWORD
                | ServerRedirectionFlags::PASSWORD_IS_PK_ENCRYPTED
                | ServerRedirectionFlags::REDIRECTION_GUID
                | ServerRedirectionFlags::TARGET_CERTIFICATE,
            target_net_address: None,
            load_balance_info: Some(b"Cookie: msts=3735928559\r\n".to_vec()),
            username: Some("handover-user".to_owned()),
            domain: None,
            password: Some(vec![0xAA, 0xBB, 0xCC]),
            target_fqdn: None,
            target_netbios_name: None,
            tsv_url: None,
            redirection_guid: Some(b"g\0u\0i\0d\0".to_vec()),
            target_certificate: Some(TargetCertificateContainer::from_certificate_der(vec![
                1, 2, 3,
            ])),
            target_net_addresses: None,
        }
    }

    // As FreeRDP's `rdp_send_pdu` lays them out, version bits included.
    fn freerdp_frame(pdu: ServerRedirectionPdu, version: u16) -> Vec<u8> {
        let header = ShareControlHeader {
            share_control_pdu: ShareControlPdu::ServerRedirect(pdu),
            pdu_source: 0,
            share_id: 0,
        };
        let mut user_data = ironrdp_core::encode_vec(&header).unwrap();
        let pdu_type = u16::from_le_bytes([user_data[2], user_data[3]]) | version;
        user_data[2..4].copy_from_slice(&pdu_type.to_le_bytes());
        ironrdp_core::encode_vec(&X224(SendDataIndication {
            initiator_id: 1002,
            channel_id: IO_CHANNEL,
            user_data: Cow::Owned(user_data),
        }))
        .unwrap()
    }

    #[test]
    fn a_freerdp_redirect_is_read_despite_its_pdu_version() {
        let pdu = freerdp_redirection(&freerdp_frame(handover(), 0x10), IO_CHANNEL).unwrap();
        assert_eq!(pdu.username.as_deref(), Some("handover-user"));
        assert_eq!(pdu.password.as_deref(), Some(&[0xAA, 0xBB, 0xCC][..]));
    }

    #[test]
    fn other_frames_are_left_to_ironrdp() {
        assert!(freerdp_redirection(&freerdp_frame(handover(), 0), IO_CHANNEL).is_none());
        assert!(freerdp_redirection(&freerdp_frame(handover(), 0x10), 1004).is_none());
        assert!(freerdp_redirection(&[3, 0, 0, 4], IO_CHANNEL).is_none());
    }

    #[test]
    fn the_handover_carries_routing_token_and_certificate() {
        let redirect = Redirect::new(handover()).unwrap();
        assert_eq!(
            redirect.routing_token.as_deref(),
            Some("Cookie: msts=3735928559")
        );
        assert_eq!(
            redirect.certificate,
            Some(pinned_certs::fingerprint(&[1, 2, 3]))
        );
        let request = redirect.connection_request().unwrap();
        let token = b"Cookie: msts=3735928559\r\n";
        assert!(request.windows(token.len()).any(|w| w == token));
        let requested = u32::from_le_bytes(request[request.len() - 4..].try_into().unwrap());
        assert_eq!(requested, SecurityProtocol::RDSTLS.bits());
    }

    #[test]
    fn a_broker_redirect_without_rdstls_is_refused() {
        let mut pdu = handover();
        pdu.flags
            .remove(ServerRedirectionFlags::PASSWORD_IS_PK_ENCRYPTED);
        assert!(matches!(Redirect::new(pdu), Err(SessionError::Protocol(_))));

        let mut pdu = handover();
        pdu.target_net_address = Some("10.0.0.2".to_owned());
        assert!(matches!(Redirect::new(pdu), Err(SessionError::Protocol(_))));
    }

    #[test]
    fn the_authentication_request_matches_ms_rdpbcgr() {
        let request = Redirect::new(handover()).unwrap().authentication_request();
        let mut expected = vec![1, 0, 2, 0, 1, 0];
        expected.extend([8, 0]);
        expected.extend(b"g\0u\0i\0d\0");
        expected.extend([28, 0]);
        expected.extend(utf16_with_nul("handover-user"));
        expected.extend([2, 0, 0, 0]);
        expected.extend([3, 0, 0xAA, 0xBB, 0xCC]);
        assert_eq!(request, expected);
    }

    #[test]
    fn rdstls_answers_are_checked() {
        assert!(check_capabilities([1, 0, 1, 0, 1, 0, 1, 0]).is_ok());
        assert!(check_capabilities([1, 0, 1, 0, 1, 0, 2, 0]).is_err());
        assert!(check_response([1, 0, 4, 0, 1, 0, 0, 0, 0, 0]).is_ok());
        assert!(matches!(
            check_response([1, 0, 4, 0, 1, 0, 0x2E, 0x05, 0, 0]),
            Err(SessionError::AuthFailed(_))
        ));
        assert!(check_response([1, 0, 2, 0, 1, 0, 0, 0, 0, 0]).is_err());
    }

    #[tokio::test]
    async fn the_exchange_runs_over_the_stream() {
        let redirect = Redirect::new(handover()).unwrap();
        let (mut client, mut server) = tokio::io::duplex(1024);
        let serve = async {
            server.write_all(&[1, 0, 1, 0, 1, 0, 1, 0]).await.unwrap();
            let mut request = vec![0; redirect.authentication_request().len()];
            server.read_exact(&mut request).await.unwrap();
            server
                .write_all(&[1, 0, 4, 0, 1, 0, 0, 0, 0, 0])
                .await
                .unwrap();
            request
        };
        let (result, request) = tokio::join!(redirect.authenticate(&mut client), serve);
        result.unwrap();
        assert_eq!(request, redirect.authentication_request());
    }
}
