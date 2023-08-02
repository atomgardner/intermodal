use crate::common::*;

use message::extended;
use message::Message;
use peer::connection::Connection;
use peer::message;
use peer::strategy::Behaviour;

#[derive(Debug)]
pub(crate) struct InfoFetcher {
  infohash: Infohash,
  conn: Connection,
  ut_metadata_message_id: u8,
  metadata_size: usize,
  info_dict: Vec<u8>,
  info: Option<Info>,
}

impl InfoFetcher {
  pub fn new(addr: &SocketAddr, infohash: Infohash) -> Result<Self> {
    let mut conn = Connection::new(addr, infohash)?;
    if !conn.supports_extension_protocol() {
      return Err(Error::PeerUtMetadataNotSupported);
    }

    conn.send_extension_handshake(extended::Handshake::default())?;
    let handshake = conn.expect_extended_handshake()?;

    let metadata_size = match handshake.metadata_size {
      Some(size) => size,
      None => return Err(Error::PeerUtMetadataMetadataSizeNotKnown),
    };
    let ut_metadata_message_id = match handshake.message_ids.get(extended::UtMetadata::NAME) {
      Some(id) => *id,
      None => return Err(Error::PeerUtMetadataNotSupported),
    };

    Ok(Self {
      conn,
      infohash,
      info_dict: Vec::new(),
      metadata_size,
      ut_metadata_message_id,
      info: None,
    })
  }

  pub fn run(mut self) -> Result<Info> {
    self.conn.send(&Message::new_extended_handshake()?)?;
    let msg = Message::new_extended(
      self.ut_metadata_message_id,
      extended::UtMetadata::request(0),
    )?;
    self.conn.send(&msg)?;

    loop {
      let msg = self.conn.recv()?;
      self.handle_message(&msg)?;
      if let Some(info) = self.info.take() {
        return Ok(info);
      }
    }
  }

  fn verify_info_dict(&mut self) -> Result<()> {
    let info = bendy::serde::de::from_bytes::<Info>(&self.info_dict)
      .context(error::PeerUtMetadataInfoDeserialize)?;
    let infohash = Infohash::from_bencoded_info_dict(
      &bendy::serde::ser::to_bytes(&info).context(error::InfoSerialize)?,
    );
    if infohash == self.infohash {
      self.info.replace(info);
      Ok(())
    } else {
      Err(Error::PeerUtMetadataWrongInfohash)
    }
  }
}

impl Behaviour for InfoFetcher {
  fn ut_metadata_data(&mut self, msg: extended::UtMetadata, payload: &[u8]) -> Result<()> {
    let piece = self.info_dict.len() / extended::UtMetadata::PIECE_LENGTH;
    if msg.piece != piece {
      return Err(Error::PeerUtMetadataWrongPiece);
    }
    // The ut_metadata::MsgType::Data payload splits into two parts,
    // 1. a bencoded UtMetadata message,
    // 2. the binary info_dict peice data.
    // Their boundary is not delimited. Bencode the message to find the piece offset.
    let piece_offset = bendy::serde::ser::to_bytes(&msg)
      .context(error::PeerMessageBencode)?
      .len();
    if payload[piece_offset..].len() > extended::UtMetadata::PIECE_LENGTH {
      return Err(Error::PeerUtMetadataPieceLength);
    }
    self.info_dict.extend_from_slice(&payload[piece_offset..]);

    match self.info_dict.len().cmp(&self.metadata_size) {
      Ordering::Equal => self.verify_info_dict(),
      Ordering::Less => {
        let msg = Message::new_extended(
          self.ut_metadata_message_id,
          extended::UtMetadata::request(piece + 1),
        )?;
        self.conn.send(&msg)
      }
      Ordering::Greater => Err(Error::PeerUtMetadataInfoLength),
    }
  }

  fn extension_handshake(&mut self, payload: &[u8]) -> Result<()> {
    let handshake: extended::Handshake = Message::from_bencode(payload)?;
    let metadata_size = match handshake.metadata_size {
      Some(size) => size,
      None => return Err(Error::PeerUtMetadataMetadataSizeNotKnown),
    };
    let ut_metadata_message_id = match handshake.message_ids.get(extended::UtMetadata::NAME) {
      Some(id) => *id,
      None => return Err(Error::PeerUtMetadataNotSupported),
    };
    self.metadata_size = metadata_size;
    self.ut_metadata_message_id = ut_metadata_message_id;
    self.info_dict.clear();
    Ok(())
  }

  fn ut_metadata_request(&mut self, _: extended::UtMetadata) -> Result<()> {
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use peer::info_seeder::InfoSeeder;

  fn new_one_piece_info() -> Info {
    Info {
      private: Some(true),
      piece_length: Bytes(9001),
      name: "foo".into(),
      source: None,
      pieces: PieceList::new(),
      mode: Mode::Single {
        md5sum: None,
        length: Bytes(1),
      },
      update_url: None,
    }
  }

  fn new_two_piece_info() -> Info {
    let mut info = new_one_piece_info();
    info.name = "a".repeat(extended::UtMetadata::PIECE_LENGTH);
    info
  }

  #[test]
  fn extension_handshake_ok() {
    let info = new_one_piece_info();
    let info_dict = bendy::serde::ser::to_bytes(&info).unwrap();
    let infohash = Infohash::from_bencoded_info_dict(&info_dict);
    let (h, a) = InfoSeeder::spawn(info, |mut s| s.send_extended_handshake());
    let fetcher = InfoFetcher::new(&a, infohash);
    assert_matches!(h.join().unwrap(), Ok(()));
    assert_matches!(fetcher, Ok(..));
  }

  #[test]
  fn extension_handshake_timeout() {
    let info = new_one_piece_info();
    let info_dict = bendy::serde::ser::to_bytes(&info).unwrap();
    let infohash = Infohash::from_bencoded_info_dict(&info_dict);
    let (h, a) = InfoSeeder::spawn(info, |_| Ok(()));
    let fetcher = InfoFetcher::new(&a, infohash);
    assert_matches!(fetcher, Err(Error::Network { .. }));
    assert_matches!(h.join().unwrap(), Ok(()));
  }

  #[test]
  fn extension_handshake_no_metadata_size() {
    let info = new_one_piece_info();
    let info_dict = bendy::serde::ser::to_bytes(&info).unwrap();
    let infohash = Infohash::from_bencoded_info_dict(&info_dict);
    let (h, a) = InfoSeeder::spawn(info, |mut s| {
      let handshake = extended::Handshake::default();
      let msg = Message::new_extended(extended::Id::Handshake.into(), handshake)?;
      s.conn.send(&msg)
    });
    let fetcher = InfoFetcher::new(&a, infohash);
    assert_matches!(fetcher, Err(Error::PeerUtMetadataMetadataSizeNotKnown));
    assert_matches!(h.join().unwrap(), Ok(()));
  }

  #[test]
  fn extension_handshake_no_ut_metadata_message() {
    let info = new_one_piece_info();
    let info_dict = bendy::serde::ser::to_bytes(&info).unwrap();
    let infohash = Infohash::from_bencoded_info_dict(&info_dict);
    let (h, a) = InfoSeeder::spawn(info, |mut s| {
      let mut handshake = extended::Handshake::new();
      handshake.metadata_size = Some(1337);
      let msg = Message::new_extended(extended::Id::Handshake.into(), handshake)?;
      s.conn.send(&msg)
    });
    let fetcher = InfoFetcher::new(&a, infohash);
    assert_matches!(fetcher, Err(Error::PeerUtMetadataNotSupported));
    assert_matches!(h.join().unwrap(), Ok(()));
  }

  #[test]
  fn one_piece_success() {
    let info = new_one_piece_info();
    let info_dict = bendy::serde::ser::to_bytes(&info).unwrap();
    let infohash = Infohash::from_bencoded_info_dict(&info_dict);
    let (_, a) = InfoSeeder::spawn_and_seed(info.clone());
    let fetcher = InfoFetcher::new(&a, infohash).unwrap();
    assert_eq!(fetcher.run().unwrap(), info);
  }

  #[test]
  fn two_piece_success() {
    let info = new_two_piece_info();
    let info_dict = bendy::serde::ser::to_bytes(&info).unwrap();
    let infohash = Infohash::from_bencoded_info_dict(&info_dict);
    let (_, a) = InfoSeeder::spawn_and_seed(info.clone());
    let fetcher = InfoFetcher::new(&a, infohash).unwrap();
    assert_eq!(fetcher.run().unwrap(), info);
  }

  #[test]
  fn bt_handshake_bad_header() {}

  #[test]
  fn handshake_infohash_mismatch() {}

  #[test]
  fn bt_connection_timeout() {}

  #[test]
  fn ut_metadata_wrong_piece_length() {}

  #[test]
  fn ut_metadata_receive_wrong_piece() {}

  #[test]
  fn receive_info_dict_with_wrong_infohash() {}

  #[test]
  fn receive_info_dict_that_fails_to_deserialize() {}

  #[test]
  fn receive_info_dict_with_wrong_metadata_size() {}
}
