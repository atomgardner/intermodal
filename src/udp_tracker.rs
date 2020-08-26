use crate::common::*;

#[derive(Clone, Copy, Debug)]
struct ConnectRequest {
  protocol_id: u64,
  action: u32,
  transaction_id: u32,
}

#[derive(Debug)]
struct ConnectResponse {
  action: u32,
  transaction_id: u32,
  connection_id: u64,
}

#[derive(Debug)]
struct AnnounceRequest {
  connection_id: u64,
  action: u32,
  transaction_id: u32,
  infohash: [u8; 20], // Infohash,
  peer_id: [u8; 20],
  downloaded: u64,
  left: u64,
  uploaded: u64,
  event: u64,
  ip_address: u32,
  num_want: u32,
  port: u16,
}

#[derive(Debug)]
struct AnnounceResponse {
  action: u32,
  transaction_id: u32,
  interval: u32,
  leechers: u32,
  seeders: u32,
}

trait Request {
  type Response: Response;

  // It seems like there're many ways to marshal these messages. I'd prefer
  // something readable and fast. Here are some options:
  //
  // 1. Cursor. With multiple write calls. ◆
  //
  // 2. IoSlice. Is there overhead to all the ::new() calls? This will probably be
  //    the winner once the write_vectored() calls diffuse out to std::net
  //
  // 3. Serde. (Already a dependency). Seems like overkill.
  //
  // 4. Binread.
  fn serialize(&self) -> Result<Vec<u8>>;
}

trait Response {
  // We leak the response length so that payloads can be parsed. This should be
  // cleaner when associated types within traits can specify named lifetime
  // parameters.
  fn deserialize(buf: &[u8]) -> Result<(Self, usize)>
  where
    Self: std::marker::Sized;
}

impl Request for ConnectRequest {
  type Response = ConnectResponse;

  fn serialize(&self) -> Result<Vec<u8>> {
    let mut msg = Cursor::new(Vec::new());

    msg.write(&self.protocol_id.to_be_bytes())?;
    msg.write(&self.action.to_be_bytes())?;
    msg.write(&self.transaction_id.to_be_bytes())?;

    Ok(msg.into_inner())
  }
}

impl Response for ConnectResponse {
  fn deserialize(buf: &[u8]) -> Result<(Self, usize)> {
    if buf.len() < mem::size_of::<ConnectResponse>() {
      return Err(Error::UdpTrackerBadResponse);
    }

    Ok((
      Self {
        action: u32::from_be_bytes(buf[0..4].try_into().unwrap()),
        transaction_id: u32::from_be_bytes(buf[4..8].try_into().unwrap()),
        connection_id: u64::from_be_bytes(buf[8..16].try_into().unwrap()),
      },
      buf.len(),
    ))
  }
}

impl Response for AnnounceResponse {
  fn deserialize(buf: &[u8]) -> Result<(Self, usize)> {
    if buf.len() < mem::size_of::<ConnectResponse>() {
      return Err(Error::UdpTrackerBadResponse);
    }

    Ok((
      AnnounceResponse {
        action: u32::from_be_bytes(buf[0..4].try_into().unwrap()),
        transaction_id: u32::from_be_bytes(buf[4..8].try_into().unwrap()),
        interval: u32::from_be_bytes(buf[8..12].try_into().unwrap()),
        leechers: u32::from_be_bytes(buf[12..16].try_into().unwrap()),
        seeders: u32::from_be_bytes(buf[16..20].try_into().unwrap()),
      },
      buf.len(),
    ))
  }
}

impl Request for AnnounceRequest {
  type Response = AnnounceResponse;

  fn serialize(&self) -> Result<Vec<u8>> {
    let mut msg = Cursor::new(Vec::new());

    msg.write(&self.connection_id.to_be_bytes())?;
    msg.write(&self.action.to_be_bytes())?;
    msg.write(&self.transaction_id.to_be_bytes())?;
    msg.write_all(&self.infohash)?;
    msg.write_all(&self.peer_id)?;
    msg.write(&self.downloaded.to_be_bytes())?;
    msg.write(&self.left.to_be_bytes())?;
    msg.write(&self.uploaded.to_be_bytes())?;
    msg.write(&self.event.to_be_bytes())?;
    msg.write(&self.ip_address.to_be_bytes())?;
    msg.write(&self.num_want.to_be_bytes())?;
    msg.write(&self.port.to_be_bytes())?;

    Ok(msg.into_inner())
  }
}

#[derive(Clone, Copy, PartialEq)]
enum State {
  Disconnected,
  Connected { id: u64, epoch: Instant },
}

pub(crate) struct UdpTrackerConn {
  peer_id: [u8; 20],
  sock: UdpSocket,
  state: State,
}

impl<'a> UdpTrackerConn {
  const UDP_TRACKER_MAGIC: u64 = 0x0000_0417_2710_1980;

  pub fn new(peer_id: [u8; 20]) -> Result<Self> {
    Ok(UdpTrackerConn {
      peer_id,
      sock: UdpSocket::bind("0.0.0.0:0")?,
      state: State::Disconnected,
    })
  }

  pub fn connect(&mut self, hostport: &'a str) -> Result<()> {
    let mut rng = rand::thread_rng();

    self.sock.connect(hostport)?;

    // TODO: self.refresh_connection_id() -> Result<()> { sub. }
    let req = ConnectRequest {
      protocol_id: Self::UDP_TRACKER_MAGIC,
      action: 0x0000,
      transaction_id: rng.gen(),
    };

    let mut buf = [0u8; mem::size_of::<ConnectResponse>()];
    let (resp, _) = self.roundtrip(&req, &mut buf)?;

    if resp.transaction_id != req.transaction_id || resp.action != req.action {
      return Err(Error::UdpTrackerBadResponse);
    }

    self.state = State::Connected {
      id: resp.connection_id,
      epoch: Instant::now(),
    };

    Ok(())
  }

  fn get_connection_id(&self) -> Result<u64> {
    match self.state {
      State::Disconnected => Err(Error::UdpConnectionIdExpired),
      // The rust compiler balks if `epoch` is `_`. But it also warns us when
      // epoch isn't used.
      #[allow(unused_variables)]
      State::Connected { id, epoch } => Ok(id),
    }
  }

  pub fn announce(&self, btinh: Infohash) -> Result<Vec<SocketAddr>> {
    let mut rng = rand::thread_rng();
    let req = AnnounceRequest {
      connection_id: self.get_connection_id()?,
      action: 0x0001,
      transaction_id: rng.gen(),
      infohash: Infohash::into(btinh),
      peer_id: self.peer_id,
      downloaded: 0x0000,
      left: u64::MAX,
      uploaded: 0x0000,
      event: 0x0000,
      ip_address: 0x0000,
      num_want: u32::MAX,
      port: self.sock.local_addr()?.port(),
    };
    let mut buf = [0u8; 8192]; // is there a BUFSIZ macro for Rust?
    let (resp, len) = self.roundtrip(&req, &mut buf)?;

    if resp.transaction_id != req.transaction_id || resp.action != req.action {
      return Err(Error::UdpTrackerBadResponse);
    }
    self.parse_compact_peer_list(&buf[mem::size_of::<AnnounceResponse>()..len])
  }

  fn roundtrip<T: Request>(&self, req: &T, rxbuf: &mut [u8]) -> Result<(T::Response, usize)> {
    let msg = req.serialize()?;
    let read = self.send_and_retry_with_backoff(&msg, rxbuf)?;

    if read == 0 {
      Err(Error::UdpConnectionExhaustedRetries)
    } else {
      T::Response::deserialize(&rxbuf[..read])
    }
  }

  // BEP15 is possibly a bit ambiguous here:
  //
  // /\ A client must use a connection ID for no longer than one minute from receipt.
  // /\ A server must accept a connection ID for no less than two minutes from advertisement.
  // /\ If a response is not received after 15 * 2 ^ n seconds, the client
  //    should retransmit the request, where n starts at 0 and is increased up
  //    to 8 (3840 seconds) after every retransmission
  fn send_and_retry_with_backoff(&self, txbuf: &'a [u8], rxbuf: &'a mut [u8]) -> Result<usize> {
    let mut len_read: usize = 0;

    for attempt in 0..=8 {
      self.sock.send(txbuf)?;
      self
        .sock
        .set_read_timeout(Some(Duration::new(15 * 2u64.pow(attempt), 0)))?;
      match self.sock.recv(rxbuf) {
        Ok(len) => {
          len_read = len;
          break;
        }

        Err(_) => {
          println!("round trip timed out, trying {} more times", 8 - attempt);

          #[allow(unused_variables)]
          if let State::Connected { id, epoch } = self.state {
            if epoch.elapsed().as_secs() > 60 {
              return Err(Error::UdpConnectionIdExpired);
            }
          }
        }
      }
    }

    if len_read == 0 {
      Err(Error::UdpConnectionExhaustedRetries)
    } else {
      Ok(len_read)
    }
  }

  // XXX: perhaps this should be in a different namespace
  fn parse_compact_peer_list(&self, addrs: &[u8]) -> Result<Vec<SocketAddr>> {
    let mut subswarm = Vec::<SocketAddr>::new();

    let stride = match self.sock.peer_addr() {
      Ok(SocketAddr::V4(_)) => 6,
      Ok(SocketAddr::V6(_)) => 18,
      Err(source) => return Err(Error::Io { source }),
    };

    for hostpost in addrs.chunks_exact(stride) {
      let (ip, port) = hostpost.split_at(stride - 2);
      let ip = match ip.len() {
        4 => IpAddr::from(std::net::Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3])),
        6 => {
          let buf: [u8; 16] = ip[0..16]
            .try_into()
            .invariant_unwrap("iterator guarantees bounds are OK");
          IpAddr::from(std::net::Ipv6Addr::from(buf))
        }
        _ => continue,
      };
      let port = u16::from_be_bytes(
        port
          .try_into()
          .invariant_unwrap("iterator guarantees bounds are OK"),
      );

      subswarm.push((ip, port).into());
    }

    Ok(subswarm)
  }
}
