use crate::common::*;

use message::extended;
use message::Message;
use peer::connection::Connection;
use peer::message;
use peer::strategy::Behaviour;

#[cfg(test)]
pub(crate) struct InfoSeeder {
  pub(crate) conn: Connection,
  pub(crate) ut_metadata_message_id: u8,
  pub(crate) metadata_size: usize,
  pub(crate) info_dict: Vec<u8>,
  pub(crate) pieces: usize,
}

impl InfoSeeder {
  pub fn new(stream: TcpStream, info: Info) -> Result<Self> {
    let info_dict = bendy::serde::ser::to_bytes(&info).context(error::InfoSerialize)?;
    let infohash = Infohash::from_bencoded_info_dict(&info_dict);
    let mut conn = Connection::from(stream, infohash)?;

    if !conn.supports_extension_protocol() {
      return Err(Error::PeerUtMetadataNotSupported);
    }

    let handshake = conn.expect_extended_handshake()?;
    let ut_metadata_message_id = match handshake.message_ids.get(extended::UtMetadata::NAME) {
      Some(id) => *id,
      None => return Err(Error::PeerUtMetadataNotSupported),
    };
    let mut pieces = info_dict.len() / extended::UtMetadata::PIECE_LENGTH;
    if info_dict.len() % extended::UtMetadata::PIECE_LENGTH > 0 {
      pieces += 1;
    }

    Ok(Self {
      conn,
      metadata_size: info_dict.len(),
      info_dict,
      ut_metadata_message_id,
      pieces,
    })
  }

  pub(crate) fn send_ut_metadata_data(&mut self, piece: usize) -> Result<()> {
    let range = std::ops::Range {
      start: extended::UtMetadata::PIECE_LENGTH * piece,
      end: if self.pieces - piece == 1 {
        extended::UtMetadata::PIECE_LENGTH * piece
          + self.info_dict.len() % extended::UtMetadata::PIECE_LENGTH
      } else {
        extended::UtMetadata::PIECE_LENGTH * (piece + 1)
      },
    };
    let msg = Message::new_extended_with_trailer(
      self.ut_metadata_message_id,
      extended::UtMetadata::data(piece, self.metadata_size),
      &self.info_dict[range],
    )?;
    self.conn.send(&msg)
  }

  pub fn spawn<W, T>(info: Info, work: W) -> (thread::JoinHandle<Result<T>>, SocketAddr)
  where
    W: Fn(InfoSeeder) -> Result<T> + Send + 'static,
    T: Send + 'static,
  {
    let _info_dict = bendy::serde::ser::to_bytes(&info).unwrap();
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
    let addr = (Ipv4Addr::LOCALHOST, listener.local_addr().unwrap().port()).into();
    let join = thread::spawn(move || {
      let (conn, _) = listener.accept().context(error::Network)?;
      conn
        .set_read_timeout(Some(Duration::new(3, 0)))
        .context(error::Network)?;
      let seeder = Self::new(conn, info).unwrap();
      work(seeder)
    });

    (join, addr)
  }

  pub fn spawn_and_seed(info: Info) -> (thread::JoinHandle<Result<()>>, SocketAddr) {
    Self::spawn(info, Self::seed)
  }

  pub fn send_extended_handshake(&mut self) -> Result<()> {
    let handshake = extended::Handshake {
      metadata_size: Some(self.info_dict.len()),
      ..extended::Handshake::default()
    };
    let msg = Message::new_extended(extended::Id::Handshake.into(), handshake)?;
    self.conn.send(&msg)
  }

  pub fn seed(mut seeder: InfoSeeder) -> Result<()> {
    // send extended handshake
    let handshake = extended::Handshake {
      metadata_size: Some(seeder.info_dict.len()),
      ..extended::Handshake::default()
    };
    let msg = Message::new_extended(extended::Id::Handshake.into(), handshake)?;
    seeder.conn.send(&msg)?;

    // Respond to any serviceable ut_metadata request. Ignore errors.
    loop {
      let msg = match seeder.conn.recv() {
        Ok(msg) => msg,
        Err(_) => continue,
      };
      if let Err(_) = seeder.handle_message(&msg) {
        continue;
      }
    }
  }

  pub fn receive_and_ignore(mut seeder: InfoSeeder) -> Result<()> {
    let handshake = extended::Handshake {
      metadata_size: Some(seeder.info_dict.len()),
      ..extended::Handshake::default()
    };
    let msg = Message::new_extended(extended::Id::Handshake.into(), handshake)?;
    seeder.conn.send(&msg)?;
    loop {
      if let Err(_) = seeder.conn.recv() {
        continue;
      }
    }
  }
}

impl Behaviour for InfoSeeder {
  fn ut_metadata_request(&mut self, m: extended::UtMetadata) -> Result<()> {
    if m.piece > self.pieces {
      return Ok(());
    }
    self.send_ut_metadata_data(m.piece)
  }

  fn ut_metadata_data(&mut self, _: extended::UtMetadata, _: &[u8]) -> Result<()> {
    Ok(())
  }

  fn extension_handshake(&mut self, _: &[u8]) -> Result<()> {
    Ok(())
  }
}
