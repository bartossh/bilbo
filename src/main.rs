use bilbo::entropy;
use clap::{Command, command, arg, value_parser};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::io::{Error, ErrorKind, Result, Write};
use std::fs::read_to_string;
use bilbo::rsa::{PickLock, to_pem, KeyType};
use bilbo::smuggler::{ping_cipher, ping_plain, Config};

const EXPLAIN: &str = "
[ 🐉 🏔 💎 ] BILBO

[ 🔐 ] Bilbo offers two RSA cracking algorithms.

1. Weak 😜:
Is cracking RSA private key when p and q are not to far apart.
Crack Weak Private is able to crack secured RSA keys, where p and q are picked to be close numbers,
Based on https://en.wikipedia.org/wiki/Fermat%27s_factorization_method
With common RSA key sizes (2048 bit) in tests,
the Fermat algorithm with 100 rounds reliably factors numbers where p and q differ up to 2^517.
In other words, it can be said that primes that only differ within the lower 64 bytes
(or around half their size) will be vulnerable.
If this tool cracks your key, you are using insecure RSA algorithm.
e - public exponent
n - modulus
d - private exponent
e and n are bytes representation of an integer in big endian order.
Returns private key as bytes representation of an integer in big endian order or error otherwise.
Will not go further then 1000 iterations.

2. Strong 💪:
Is cracking RSA when p and q are far apart.
Similar in terms of factorization to weak algorithm, but works on the principal that RSA p and q
are chosen according to the specification where:
 -> p * q = n,
 -> p and q are primes that differs in more than first 2^517 - last 64 bytes.
 -> p and q are fairly equal in bits size and can vary +/- 1 bit,
 -> bits size of p + bits size of q are equal to n,

[ 🧮 ] Bilbo offers entropy calculation.

The Shannon entropy is a statistical quantifier extensively used for the characterization of complex processes. It is capable of detecting nonlinearity aspects in model series, contributing to a more reliable explanation regarding the nonlinear dynamics of different points of analysis, which in turn enhances the comprehension of the nature of complex systems characterized by complexity and nonequilibrium.
In addition to complexity and nonequilibrium, most, not all, complex systems also have the characteristic of being marked by heterogeneous distributions of links.
The concept of entropy was used by Shannon in information theory for the data communication of computer sciences and is known as Shannon entropy.
Based on this concept, the mean value of the shortest possibilities required to code a message is the division of the symbol logarithm in the alphabet by the entropy.
Entropy refers to a measurement of vagueness and randomness in a system. If we assume that all the available data belong to one class, it will not be hard to predict the class of a new data.

[ 📦 ] Message smuggler via ping.

The message smuggler via ping allows to smuggle message in plain text or encrypted message with 16 bytes long key via ping.
Smuggler may be useful when proxy blocks internet traffic but allows ping and you want to send message outside.
Encryption used for the message is EAS and encrypts 16 bytes long blocks that are collected in to buffer and then
sent via ping in 24 bytes long chunks. The initialization vector is transferred on the end of communication in plaintext.
";

fn main() {
    let cmd = Command::new("bilbo")
        .bin_name("bilbo")
        .subcommand_required(true)
        .about("🧝 Bilbo is a simple CLI cyber security tool. Scans files to discover hidden information and helps send them secretly.")
        .subcommand(
            command!("smuggle")
            .about("Smuggles the file via ping.")
            .arg(
                arg!(--"file" <FILE> "Path to file in PEM format to be smuggled")
                    .value_parser(value_parser!(PathBuf)),
            ).arg(
                arg!(--"ip" <IP> "IPv4 to the server that will collect smuggled file.").value_parser(value_parser!(Ipv4Addr)),
            ).arg(
                arg!(--"encrypt" <KEY> "Encryption key.").value_parser(value_parser!(Vec<u8>)),
            )
        )
        .subcommand(
            command!("picklock")
            .about("Attempts to pick lock the rsa key.")
            .arg(
                arg!(--"file" <FILE> "Path to file in PEM format to be lock picked")
                    .value_parser(value_parser!(PathBuf)),
            ).arg(
                arg!(--"strong" <ITERS> "Number of primes to iterate over. Primes are randomly generated").value_parser(value_parser!(u32)),
            ).arg(
                arg!(--"report" <LEVEL> "Level of reporting. 0 (default): Only results. 1: Important steps only. 2: Information about number of primes checked.").value_parser(value_parser!(u8)),
            ),
        ).subcommand(
            command!("explain"). about("Explains used algorithms."),
        ).subcommand(
            command!("entropy")
            .about("Calculates Shannon entropy for file content per line and total entropy of a file.")
            .arg(
                arg!(--"file" <FILE> "Path to file.")
                    .value_parser(value_parser!(PathBuf)),
            ).arg(
                arg!(--"report" <LEVEL> "Level of reporting. 0 (default): Only results. 1: Important steps only. 2: All foundings such as each line entropy.").value_parser(value_parser!(u8)),
            )
        );
    let matches = cmd.get_matches();
    match matches.subcommand() {
        Some(("picklock", matches)) =>  {
            match run_picklock(matches.get_one::<PathBuf>("file"),
            matches.get_one::<u32>("strong"), matches.get_one::<u8>("report")) {
                Ok(s) => println!("🗝 Lock picked private PEM key:\n{s}\n"),
                Err(e) => println!("🤷 Failure: {}", e.to_string()),
            }
        },
        Some(("entropy", matches)) => {
            match run_entropy(matches.get_one::<PathBuf>("file"), matches.get_one::<u8>("report")) {
                Ok(s) => println!("📶 Entropy:\n{s}\n"),
                Err(e) => println!("🤷 Failure: {}", e.to_string()),
            }

        },
        Some(("smuggle", matches)) => match smuggle_file_via_ping(matches.get_one("file"), matches.get_one("ip"), matches.get_one("encrypt")) {
            Ok(s) => println!("📦 Ping Smuggler: \n{s}\n"),
            Err(e) => println!("🤷 Failure: {}", e.to_string()),
        }
        Some(("explain", _matches)) => println!("{EXPLAIN}"),
        None => (),
        _ => unreachable!("unreachable code"),
    };
}

fn run_picklock(path: Option<&PathBuf>, strong_iters: Option<&u32>, report_level: Option<&u8>) -> Result<String> {
    let report_level = check_level(report_level)?;
    let Some(path) = path else {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "I received an empty file path... I don't know what to picklock, please be specific..."
        ))
    };

    let rsa_pem = read_to_string(path)?;
    let mut pl = PickLock::from_pem(&rsa_pem)?;

    let d = match strong_iters {
        None => {
            if report_level >= 1 {
                println!("🔐 Starting lock picking the weak RSA private key.\n");
            }
            pl.try_lock_pick_weak_private()?
        },
        Some(iter) => {
            if report_level >= 1 {
                println!("🔐 Starting lock picking the strong RSA private key.\n");
            }
            if *iter != 0 {
                pl.alter_max_iter(*iter as usize)?;
            }
            pl.try_lock_pick_strong_private(report_level == 2)?
        }
    };
    let pem_priv = to_pem(d, KeyType::Private)?;

    Ok(pem_priv)
}

fn run_entropy(path: Option<&PathBuf>, report_level: Option<&u8>) -> Result<String> {
    let report_level = check_level(report_level)?;
    if report_level >= 1 {
        println!("🧮 Starting Shannon entropy calculation.\n");
    }
    let Some(path) = path else {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "I received an empty file path... I don't know what file to calculate entropy for, please be specific..."
        ))
    };

    let data = read_to_string(path)?;

    let mut result = String::new();
    let mut total_entropy = entropy::Shannon::new();
    let mut total_bts: usize = 0;

    result.push_str(&format!(
        "| {0: <6} | {1: <8} | {2: <7} | {3: <5} | {4: <24} |\n",
        "Line", "Entropy", "Bytes", "Ratio", "Starts with"
    ));
    result.push_str(&format!(
        "|================================================================|\n",
    ));

    for (i, line) in data.lines().enumerate() {
        let mut ent = entropy::Shannon::new();
        let buf = line.as_bytes();

        total_entropy.write(buf)?;
        total_entropy.process();
        let bts = buf.len();
        total_bts += bts;
        if report_level == 2 {
            ent.write(buf)?;
            ent.process();
            let e = ent.get_entropy();
            let ratio = if bts == 0 { 0 } else {e / bts as u64};
            result.push_str(&format!("| {0: <6} | {1: <8} | {2: <7} | {3: <5} | {4: <21}... |\n", i+1, e, bts, ratio, &line[..if line.len() < 21 { line.len() } else { 21 }]));
        }
    }

    let total_entropy = total_entropy.get_entropy();
    let ratio = if total_bts == 0 { 0 } else {total_entropy / total_bts as u64};

    if report_level == 2 {
        result.push_str(&format!(
            "|================================================================|\n",
        ));
    }
    result.push_str(&format!("| {0: <6} | {1: <8} | {2: <7} | {3: <5} | {4: <24} |\n", "TOTAL", total_entropy, total_bts, ratio, "         ---"));

    Ok(result)
}

fn smuggle_file_via_ping(file: Option<&PathBuf>, ip: Option<&Ipv4Addr>, key: Option<&Vec<u8>>) -> Result<String> {
    let Some(path) = file else {
        return Err(Error::new(ErrorKind::InvalidInput, "empty or incorrect file path"));
    };
    let Some(ip) = ip else {
        return Err(Error::new(ErrorKind::InvalidInput, "empty or incorrect ip address"));
    };

    let data = read_to_string(path)?;

    match key {
        None => {
            ping_plain(IpAddr::V4(*ip), &data.as_bytes(), &Config::default())?;
            Ok(format!("File {:?} smuggled to {}\n", path.as_os_str(), ip.to_string()).to_string())
        },
        Some(k) => {
            if k.len() != 16 {
                return Err(
                    Error::new(ErrorKind::InvalidInput,
                    format!("incorrect kye size, expected 16 bytes, got {} bytes", k.len())),
                );
            }

            let cfg = Config::default();
            let mut enc_key: [u8;16] = [0;16];
            for i in 0..16 {
                enc_key[i] = k[i];
            }

            let ip = IpAddr::V4(*ip);
            let vi = ping_cipher(ip, &data.as_bytes(), &enc_key, &cfg)?;
            ping_plain(ip, &vi, &cfg)?;

            Ok(format!("File {:?} smuggled to {}, with IV: {:?} \n", path.as_os_str(), ip.to_string(), vi).to_string())
        },
    }
}

fn check_level(level: Option<&u8>) -> Result<u8> {
    let level = *level.unwrap_or(&0);
    match level {
        0 | 1 | 2 => Ok(level),
        _ => Err(Error::new(ErrorKind::InvalidData, format!("Expected level 0, 1 or 2, got {level}"))),
    }
}
