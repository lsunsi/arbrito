use num_bigint::{BigInt, Sign};
use web3::types::U256;

fn root(ri: U256, ro: U256, bi: U256, bo: U256, s: U256) -> Option<U256> {
    let mut buffer: [u8; 32] = [0; 32];

    bi.to_little_endian(&mut buffer);
    let bi = BigInt::from_bytes_le(Sign::Plus, &buffer);

    bo.to_little_endian(&mut buffer);
    let bo = BigInt::from_bytes_le(Sign::Plus, &buffer);

    ri.to_little_endian(&mut buffer);
    let ri = BigInt::from_bytes_le(Sign::Plus, &buffer);

    ro.to_little_endian(&mut buffer);
    let ro = BigInt::from_bytes_le(Sign::Plus, &buffer);

    s.to_little_endian(&mut buffer);
    let s = BigInt::from_bytes_le(Sign::Plus, &buffer);

    let bone = BigInt::from(1000000000000000000u128);
    let tho = BigInt::from(1000);
    let nns = BigInt::from(997);
    let two = BigInt::from(2);

    let a = (&bi * &bo * &nns / &tho + &ri * &ro * &two * &s / &bone)
        - (&ri * &ro + &bi * &bo * &nns * &s / (&tho * &bone) + &ri * &ro * s.pow(2) / bone.pow(2));

    let b = (&bi * &ri * &ro * &two * &s / &bone
        + &bi * &bo * &ro * &two * &nns * &s / (&tho * &bone))
        - (&bi * &ri * &ro * &two + &bi * &bo * &ro * &two * &nns / &tho);

    let c = (&bi * &bo * ro.pow(2) * &nns / &tho)
        - (bi.pow(2) * &ri * &ro + &bi * &bo * ro.pow(2) * &nns * &s / (&tho * &bone));

    let delta = b.pow(2) - &a * &c * two.pow(2);

    if delta.sign() == Sign::Minus || a.sign() == Sign::NoSign {
        return None;
    }

    let root0 = (-&b + delta.sqrt()) / (&a * &two);
    let root1 = (-&b - delta.sqrt()) / (&a * &two);

    let viable = |x: &BigInt| x.sign() == Sign::Plus && x <= &ro;
    let to_u256 = |x: BigInt| U256::from_little_endian(&x.to_bytes_le().1);

    match (viable(&root0), viable(&root1)) {
        (false, false) => None,
        (false, true) => Some(to_u256(root1)),
        (true, false) => Some(to_u256(root0)),
        (true, true) => {
            log::error!(
                "Two viable roots?!. ri={} ro={} bi={} bo={} s={}",
                ri,
                ro,
                bi,
                bo,
                s
            );
            None
        }
    }
}

fn uniswap_in_given_out(ri: U256, ro: U256, amount: U256) -> U256 {
    (amount * U256::from(ri) * 1000) / ((U256::from(ro) - amount) * 997) + 1
}

fn balancer_out_given_in(bi: U256, bo: U256, s: U256, amount: U256) -> U256 {
    let bone = U256::exp10(18);

    let bmul = |a: U256, b: U256| (a * b + bone / 2) / bone;
    let bdiv = |a: U256, b: U256| (a * bone + b / 2) / b;

    bmul(bo, bone - bdiv(bi, bi + bmul(amount, bone - s)))
}

fn profit(ri: U256, ro: U256, bi: U256, bo: U256, s: U256, amount: U256) -> Option<U256> {
    balancer_out_given_in(bi, bo, s, amount).checked_sub(uniswap_in_given_out(ri, ro, amount))
}

pub fn max_profit(ri: U256, ro: U256, bi: U256, bo: U256, s: U256) -> Option<(U256, U256)> {
    let amount = root(ri, ro, bi, bo, s)?;
    let profit = profit(ri, ro, bi, bo, s, amount)?;

    Some((amount, profit))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn works() {
        let ro = U256::from(560407980246u128);
        let ri = U256::from(185214260915118229728572u128);
        let bo = U256::from(674650730267410526933u128);
        let bi = U256::from(2032847980u128);
        let s = U256::from(300000000000000u128);

        let (amount, profit) = max_profit(ri, ro, bi, bo, s).unwrap();

        assert_eq!(amount, U256::from(860531u128));
        assert_eq!(profit, U256::from(121209478698546u128));
    }
}
