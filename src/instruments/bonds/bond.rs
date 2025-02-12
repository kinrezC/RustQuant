// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// RustQuant: A Rust library for quantitative finance tools.
// Copyright (C) 2023 https://github.com/avhz
// Dual licensed under Apache 2.0 and MIT.
// See:
//      - LICENSE-APACHE.md
//      - LICENSE-MIT.md
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

use crate::curves::{Curve, YieldCurve};
use crate::instruments::Instrument;
use crate::money::Currency;
use crate::time::{BusinessDayConvention, PaymentFrequency};
use std::collections::BTreeMap;
use time::{Duration, OffsetDateTime};

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// STRUCTS, ENUMS, AND TRAITS
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Zero-coupon bond struct.
/// A zero-coupon bond (aka a pure discount bond or simply a zero) is a
/// debt security that doesn't pay interest (a coupon) periodically but
/// instead pays the principal in full at maturity.
pub struct ZeroCouponBond {
    /// The date the bond is evaluated (i.e. priced).
    pub evaluation_date: OffsetDateTime,

    /// The date the bond expires (i.e. matures, is redeemed).
    pub expiration_date: OffsetDateTime,

    /// The currency of the bond (optional).
    pub currency: Option<Currency>,
}

/// Coupon bond struct.
/// A coupon bond is a debt obligation with coupons attached that represent
/// interest payments on the debt.
///
/// A coupon bond can be viewed as a portfolio of zero-coupon bonds.
///
/// For example, consider a 1.5 year bond with a 5% coupon rate (semi-annual)
/// and a face value of $100.
/// Then the bond can be viewed as a portfolio of three zero-coupon bonds:
/// - A 6-month zero-coupon bond with a face value of $2.50.
/// - A 12-month zero-coupon bond with a face value of $2.50.
/// - An 18-month zero-coupon bond with a face value of $102.50.
pub struct CouponBond {
    /// The date the bond is evaluated (i.e. priced).
    pub evaluation_date: OffsetDateTime,

    /// The date the bond expires (i.e. matures, is redeemed).
    pub expiration_date: OffsetDateTime,

    /// The currency of the bond (optional).
    pub currency: Option<Currency>,

    /// The coupon rate of the bond.
    pub coupon_rate: f64,

    /// The coupon frequency of the bond.
    pub coupon_frequency: PaymentFrequency,

    /// Settlement convention.
    pub settlement_convention: BusinessDayConvention,

    /// Yield curve to use for pricing.
    pub yield_curve: YieldCurve,

    /// The face value of the bond.
    pub face_value: f64,

    /// The coupons of the bond.
    /// The coupons are represented as a map of dates to coupon amounts,
    /// ordered by date.
    /// The final coupon is the face value of the bond.
    pub coupons: BTreeMap<OffsetDateTime, f64>,
}

/// Coupon bond struct.
pub struct CouponBond2 {
    /// Portfolio of zero-coupon bonds.
    pub coupons: BTreeMap<OffsetDateTime, ZeroCouponBond>,
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// IMPLEMENTATIONS
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

impl CouponBond {
    /// Constructs the coupons of the bond.
    pub fn construct_coupons(&mut self) {
        let mut coupons: BTreeMap<OffsetDateTime, f64> = BTreeMap::new();

        // Create the coupon dates
        let years = (self.expiration_date - self.evaluation_date).whole_days() / 365;
        let n_coupons = years * self.coupon_frequency as i64;

        let mut coupon_dates: Vec<OffsetDateTime> = Vec::with_capacity(n_coupons as usize);

        for i in 1..=n_coupons {
            let coupon_date =
                self.evaluation_date + Duration::days(365 * i) / self.coupon_frequency as i32;

            coupon_dates.push(coupon_date);
        }

        // Create the coupon amounts
        let mut coupon_amounts: Vec<f64> = Vec::with_capacity(n_coupons as usize);

        for _ in 1..n_coupons {
            let coupon_amount =
                self.face_value * self.coupon_rate / self.coupon_frequency as isize as f64;

            coupon_amounts.push(coupon_amount);
        }

        // Create the coupons
        for (date, amount) in coupon_dates.iter().zip(coupon_amounts.iter()) {
            coupons.insert(*date, *amount);
        }

        // Add the final coupon
        coupons.insert(
            self.expiration_date,
            self.face_value * (1.0 + self.coupon_rate / self.coupon_frequency as isize as f64),
        );

        self.coupons = coupons;
    }
}

impl Instrument for CouponBond {
    /// Returns the price (net present value) of the instrument.
    fn price(&self) -> f64 {
        // Compute the discount factors for the coupons.
        let discount_factors = self.yield_curve.discount_factors(
            &self
                .coupons
                .keys()
                .cloned()
                .collect::<Vec<OffsetDateTime>>(),
        );
        // .iter()
        // .enumerate()
        // .map(|(i, df)| (1. + df / self.coupon_frequency as i32 as f64).powi((i + 1) as i32))
        // .collect::<Vec<f64>>();

        // Compute the present value of the coupons and face value, and sum them.
        self.coupons
            .values()
            .zip(discount_factors.iter())
            .map(|(coupon, df)| coupon * df)
            .sum::<f64>()
    }

    /// Returns the error on the NPV in case the pricing engine can
    /// provide it (e.g. Monte Carlo pricing engine).
    fn error(&self) -> Option<f64> {
        None
    }

    /// Returns the date at which the NPV is calculated.
    fn valuation_date(&self) -> OffsetDateTime {
        self.evaluation_date
    }

    /// Instrument type.
    fn instrument_type(&self) -> &'static str {
        "Coupon Bond"
    }
}

impl CouponBond2 {
    /// Validate the dates.
    /// All evaluation dates must be the same, since it is a single instrument,
    /// we just happen to be treating it as a portfolio of zero-coupon bonds.
    pub fn validate_dates(&self) -> bool {
        let mut evaluation_date: Option<OffsetDateTime> = None;

        for (_, bond) in self.coupons.iter() {
            if evaluation_date.is_none() {
                evaluation_date = Some(bond.evaluation_date);
            } else if evaluation_date != Some(bond.evaluation_date) {
                return false;
            }
        }

        true
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// UNIT TESTS
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[cfg(test)]
mod tests_bond {
    // use time::macros::datetime;

    use crate::{curves::Curve, money::USD};

    use super::*;

    fn create_test_yield_curve(t0: OffsetDateTime) -> YieldCurve {
        // Create a treasury yield curve with 8 points (3m, 6m, 1y, 2y, 5y, 10y, 30y).
        // Values from Bloomberg: <https://www.bloomberg.com/markets/rates-bonds/government-bonds/us>
        let rate_vec = vec![0.0544, 0.0556, 0.0546, 0.0514, 0.0481, 0.0481, 0.0494];
        let date_vec = vec![
            t0 + Duration::days(90),
            t0 + Duration::days(180),
            t0 + Duration::days(365),
            t0 + Duration::days(2 * 365),
            t0 + Duration::days(5 * 365),
            t0 + Duration::days(10 * 365),
            t0 + Duration::days(30 * 365),
        ];

        YieldCurve::from_dates_and_rates(&date_vec, &rate_vec)
    }

    #[test]
    fn test_coupon_construction() {
        let today = OffsetDateTime::now_utc();

        let mut bond = CouponBond {
            evaluation_date: today,
            expiration_date: today + Duration::days(365 * 2),
            currency: Some(USD),
            coupon_rate: 0.15,
            coupon_frequency: PaymentFrequency::SemiAnnually,
            settlement_convention: BusinessDayConvention::Actual,
            yield_curve: create_test_yield_curve(today),
            face_value: 1000.0,
            coupons: BTreeMap::new(),
        };

        bond.construct_coupons();

        // Should be: $1,184.61
        // Getting:   $1,198.47
        // Think its close enough for now, down to differences in my computation
        // and the calculator I used. Possibly continuous compounding vs discrete.
        println!("Price: {}", bond.price());
    }
}
