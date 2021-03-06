//! same solution as in src/lib.rs, but preaggregating the 'gamma' points
//! note: now the gamma points are not added into the 'c' hash calculation
#![allow(non_snake_case)]

use rand::prelude::*;
use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{RistrettoPoint},
    scalar::Scalar,
    traits::Identity
};
use std::ops::{Deref, Neg};
use super::{ECVRF_hash_to_curve, ECVRF_hash_points, ECVRF_nonce, add_points};

pub struct KeyPair {
  private: Scalar,
  public:  RistrettoPoint,
}

impl KeyPair {
  pub fn new<T: Rng + CryptoRng>(rng: &mut T) -> Self {
    let private = Scalar::random(rng);
    let public = private * RISTRETTO_BASEPOINT_POINT;

    KeyPair { private, public }
  }
}

pub struct Token {
  pub messages: Vec<Vec<u8>>,
  pub keys: Vec<RistrettoPoint>,
  pub signature: TokenSignature,
}

impl Token {
  pub fn new(keypair: &KeyPair, message: &[u8]) -> Self {
    let signature = TokenSignature::new(keypair, message);

    Token {
      messages: vec![message.to_owned()],
      keys: vec![keypair.public],
      signature
    }
  }

  pub fn append(&self, keypair: &KeyPair, message: &[u8]) -> Self {
    let signature = self.signature.sign(&self.keys, &self.messages, keypair, message);

    let mut t = Token {
      messages: self.messages.clone(),
      keys: self.keys.clone(),
      signature
    };

    t.messages.push(message.to_owned());
    t.keys.push(keypair.public);

    t
  }

  pub fn verify(&self) -> bool {
    self.signature.verify(&self.keys, &self.messages)
  }
}

pub struct TokenSignature {
  gamma_agg: RistrettoPoint,
  c: Vec<Scalar>,
  w: RistrettoPoint,
  s: Scalar
}

impl TokenSignature {
  pub fn new(keypair: &KeyPair, message: &[u8]) -> Self {
    let h = ECVRF_hash_to_curve(keypair.public, message);
    let gamma = keypair.private * h;
    let k = ECVRF_nonce(keypair.private, h);
    let c = ECVRF_hash_points(&[h, keypair.public,  k* RISTRETTO_BASEPOINT_POINT, k*h]);
    let s = (k + c * keypair.private).reduce();

    // W = h^(s0 - S) * .. * hn^(sn - S)
    let w = RistrettoPoint::identity();

    TokenSignature {
      gamma_agg: c.neg() * gamma,
      c: vec![c],
      w,
      s
    }
  }

  pub fn sign<M: Deref<Target=[u8]>>(&self, public_keys: &[RistrettoPoint],
    messages: &[M], keypair: &KeyPair, message: &[u8]) -> Self {
    let h = ECVRF_hash_to_curve(keypair.public, message);
    let gamma = keypair.private * h;
    let k = ECVRF_nonce(keypair.private, h);

    let pc = public_keys.iter().zip(self.c.iter()).map(|(p, c)| p*(c.neg())).collect::<Vec<_>>();
    // u = g^(k0 + k1 + ... + kn)
    let u = add_points(&pc)  + (self.s * RISTRETTO_BASEPOINT_POINT) + (k * RISTRETTO_BASEPOINT_POINT);

    let hashes = messages.iter().zip(public_keys.iter()).map(|(m, pk)| ECVRF_hash_to_curve(*pk, m)).collect::<Vec<_>>();
    let hashes_sum = add_points(&hashes);

    // v = h0^k0 * h1^k1 * .. * hn^k^n
    let v = self.w + self.gamma_agg + (self.s * hashes_sum) + (k * h);

    let p = add_points(public_keys);

    let c = ECVRF_hash_points(&[h, p + keypair.public,  u, v]);

    let s = (k + c * keypair.private).reduce();
    let agg_s = (self.s + s).reduce();

    let hs = hashes_sum * s.neg();
    let w = self.w + hs + h * self.s.neg();

    let mut res = TokenSignature {
      gamma_agg: self.gamma_agg + c.neg() * gamma,
      c: self.c.clone(),
      w,
      s: agg_s
    };
    res.c.push(c);

    res
  }

  pub fn verify<M: Deref<Target=[u8]>>(&self, public_keys: &[RistrettoPoint], messages: &[M]) -> bool {
    if !(public_keys.len() == messages.len()
         && public_keys.len() == self.c.len()) {
      println!("invalid data");
      return false;
    }

    let pc = public_keys.iter().zip(self.c.iter()).map(|(p, c)| p*c.neg()).collect::<Vec<_>>();
    // u = g^(k0 + k1 + ... + kn)
    let u = add_points(&pc) + (self.s *RISTRETTO_BASEPOINT_POINT);

    let hashes = messages.iter().zip(public_keys.iter()).map(|(m, pk)| ECVRF_hash_to_curve(*pk, m)).collect::<Vec<_>>();
    let hashes_sum = add_points(&hashes);

    let v = self.w + self.gamma_agg + (self.s * hashes_sum);

    let p = add_points(public_keys);

    let c = ECVRF_hash_points(&[*hashes.last().unwrap(), p, u, v]);

    c == *self.c.last().unwrap()
  }

}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn three_messages() {
    //let mut rng: OsRng = OsRng::new().unwrap();
    //keep the same values in tests
    let mut rng: StdRng = SeedableRng::seed_from_u64(0);

    let message1 = b"hello";
    let keypair1 = KeyPair::new(&mut rng);

    let token1 = Token::new(&keypair1, &message1[..]);

    assert!(token1.verify(), "cannot verify first token");

    println!("will derive a second token");

    let message2 = b"world";
    let keypair2 = KeyPair::new(&mut rng);

    let token2 = token1.append(&keypair2, &message2[..]);

    assert!(token2.verify(), "cannot verify second token");

    println!("will derive a third token");

    let message3 = b"!!!";
    let keypair3 = KeyPair::new(&mut rng);

    let token3 = token2.append(&keypair3, &message3[..]);

    assert!(token3.verify(), "cannot verify third token");
  }

  #[test]
  fn change_message() {
    //let mut rng: OsRng = OsRng::new().unwrap();
    //keep the same values in tests
    let mut rng: StdRng = SeedableRng::seed_from_u64(0);

    let message1 = b"hello";
    let keypair1 = KeyPair::new(&mut rng);

    let token1 = Token::new(&keypair1, &message1[..]);

    assert!(token1.verify(), "cannot verify first token");

    println!("will derive a second token");

    let message2 = b"world";
    let keypair2 = KeyPair::new(&mut rng);

    let mut token2 = token1.append(&keypair2, &message2[..]);

    token2.messages[1] = Vec::from(&b"you"[..]);

    assert!(!token2.verify(), "second token should not be valid");

    println!("will derive a third token");

    let message3 = b"!!!";
    let keypair3 = KeyPair::new(&mut rng);

    let token3 = token2.append(&keypair3, &message3[..]);

    assert!(!token3.verify(), "cannot verify third token");
  }
}
