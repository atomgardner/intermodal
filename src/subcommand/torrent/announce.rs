use crate::common::*;

const URI_HELP: &str = "Announce an infohash and list the response (a compact peer list).";

const INPUT_HELP: &str =
  "Generate a compact peer list from a metainfo at `INPUT`. If `INPUT` is `-`, read \
                          metainfo from standard input.";

const INPUT_FLAG: &str = "input-flag";

const INPUT_POSITIONAL: &str = "<INPUT>";

#[derive(StructOpt)]
#[structopt(
  help_message(consts::HELP_MESSAGE),
  version_message(consts::VERSION_MESSAGE),
  about(URI_HELP)
)]
pub(crate) struct Announce {
  #[structopt(
    name = INPUT_FLAG,
    long = "input",
    short = "i",
    value_name = "INPUT",
    empty_values(false),
    parse(try_from_os_str = InputTarget::try_from_os_str),
    help = INPUT_HELP,
  )]
  input_flag: Option<InputTarget>,
  #[structopt(
    name = INPUT_POSITIONAL,
    value_name = "INPUT",
    empty_values(false),
    parse(try_from_os_str = InputTarget::try_from_os_str),
    required_unless = INPUT_FLAG,
    conflicts_with = INPUT_FLAG,
    help = INPUT_HELP,
  )]
  input_positional: Option<InputTarget>,
}

impl Announce {
  pub(crate) fn run(self, env: &mut Env, options: &Options) -> Result<(), Error> {
    let target = xor_args(
      "input_flag",
      &self.input_flag,
      "input_positional",
      &self.input_positional,
    )?;
    let input = env.read(target)?;

    let infohash = Infohash::from_input(&input)?;
    let metainfo = Metainfo::from_input(&input)?;

    let mut rng = rand::thread_rng();
    let peer_id: [u8; 20] = rng.gen();
    let mut peer_list = Vec::new();

    if metainfo.trackers().peekable().peek().is_none() {
      if !options.quiet {
        println!("Supplied metainfo specifies no trackers.");
      }
      return Err(Error::NoPeerSource);
    }

    if !options.quiet {
      println!("[1/2] Announcing {} to trackers.", &infohash);
    }

    for tr in metainfo.trackers() {
      let tracker = match tr {
        Ok(tr) => tr,
        Err(err) => {
          if !options.quiet {
            println!("{:?}", err);
          }
          continue;
        }
      };

      match tracker.scheme() {
        "udp" => {
          let hostport = tracker.into_string();

          if !options.quiet {
            println!("[1/2] Sending announce to {}.", hostport);
          }
          let mut conn = UdpTrackerConn::new(peer_id)?;
          conn.connect(hostport.trim_start_matches("udp://"))?;
          match conn.announce(infohash) {
            Ok(subswarm) => peer_list.extend(subswarm),
            Err(err) => println!("{:?}", err),
          }
        }

        _ => {
          if !options.quiet {
            println!(
              "<info> Only UDP trackers are supported at present; skipping {}.",
              tracker
            );
          }
        }
      }
    }

    if !options.quiet {
      println!("[2/2] Done");
    }

    for p in &peer_list {
      println!("{}", p);
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn input_required() {
    test_env! {
      args: [
        "torrent",
        "announce",
      ],
      tree: {
      },
      matches: Err(Error::Clap { .. }),
    };
  }
}
