//! 财务期间值对象。

use chrono::NaiveDate;

use crate::error::FinanceError;

/// 财务期间：年 + 月份范围，承载报表时间区间计算。
///
/// 构造时完成默认值（完全缺省 → 全年；只给起始月 → 该月单月）、合法性校验和日期区间计算；
/// 起始/结束日期在构造期一次性算好并缓存，构造成功后 `start_date` / `end_date` / `label`
/// 是纯取值，不可能失败——所有 fallible 的日期运算都留在构造期的 `Result` 上下文中。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FiscalPeriod {
    year: i32,
    month_start: u32,
    month_end: u32,
    start: NaiveDate,
    end: NaiveDate,
}

impl FiscalPeriod {
    /// 从查询参数构造。月份缺省语义：完全不传 → 全年（1~12 月）；只传起始月 → 该月单月。
    ///
    /// 失败条件：`year < 1`、月份不在 `1..=12`、或 `month_end < month_start`。
    pub fn try_new(
        year: i32,
        month_start: Option<u32>,
        month_end: Option<u32>,
    ) -> Result<Self, FinanceError> {
        if year < 1 {
            return Err(FinanceError::InvalidPeriod);
        }
        let month_start_given = month_start.is_some();
        let month_start = month_start.unwrap_or(1);
        // 缺省语义：完全缺省 → 全年（12 月）；只给起始月 → 该月单月
        let month_end = month_end.unwrap_or(if month_start_given { month_start } else { 12 });
        if !(1..=12).contains(&month_start)
            || !(1..=12).contains(&month_end)
            || month_end < month_start
        {
            return Err(FinanceError::InvalidPeriod);
        }

        let start =
            NaiveDate::from_ymd_opt(year, month_start, 1).ok_or(FinanceError::InvalidPeriod)?;
        let end = if month_end == 12 {
            NaiveDate::from_ymd_opt(year, 12, 31).ok_or(FinanceError::InvalidPeriod)?
        } else {
            NaiveDate::from_ymd_opt(year, month_end + 1, 1)
                .ok_or(FinanceError::InvalidPeriod)?
                .pred_opt()
                .ok_or(FinanceError::InvalidPeriod)?
        };
        Ok(Self {
            year,
            month_start,
            month_end,
            start,
            end,
        })
    }

    /// 起始日期：起始月的第一天。
    pub fn start_date(&self) -> NaiveDate {
        self.start
    }

    /// 结束日期：结束月的最后一天（自动处理月尾 / 闰年 / 12 月）。
    pub fn end_date(&self) -> NaiveDate {
        self.end
    }

    /// 期间标签：单月 `"2026-07"`，多月 `"2026-01~2026-12"`。
    pub fn label(&self) -> String {
        if self.month_start == self.month_end {
            format!("{}-{:02}", self.year, self.month_start)
        } else {
            format!(
                "{}-{:02}~{}-{:02}",
                self.year, self.month_start, self.year, self.month_end
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_whole_year() {
        let p = FiscalPeriod::try_new(2026, None, None).unwrap();
        assert_eq!(p.month_start, 1);
        assert_eq!(p.month_end, 12);
        assert_eq!(p.label(), "2026-01~2026-12");
    }

    #[test]
    fn test_defaults_end_equals_start() {
        let p = FiscalPeriod::try_new(2026, Some(7), None).unwrap();
        assert_eq!(p.month_start, 7);
        assert_eq!(p.month_end, 7);
    }

    #[test]
    fn test_valid_range() {
        assert!(FiscalPeriod::try_new(2026, Some(1), Some(12)).is_ok());
        assert!(FiscalPeriod::try_new(2026, Some(5), Some(5)).is_ok());
    }

    #[test]
    fn test_invalid_inputs() {
        assert!(FiscalPeriod::try_new(0, Some(1), Some(12)).is_err());
        assert!(FiscalPeriod::try_new(2026, Some(0), None).is_err());
        assert!(FiscalPeriod::try_new(2026, Some(13), None).is_err());
        assert!(FiscalPeriod::try_new(2026, Some(12), Some(13)).is_err());
        assert!(FiscalPeriod::try_new(2026, Some(5), Some(3)).is_err());
    }

    #[test]
    fn test_start_date_first_of_month() {
        let p = FiscalPeriod::try_new(2026, Some(7), Some(7)).unwrap();
        assert_eq!(p.start_date(), NaiveDate::from_ymd_opt(2026, 7, 1).unwrap());
    }

    #[test]
    fn test_end_date_month_end() {
        let p = FiscalPeriod::try_new(2026, Some(7), Some(7)).unwrap();
        assert_eq!(p.end_date(), NaiveDate::from_ymd_opt(2026, 7, 31).unwrap());
    }

    #[test]
    fn test_end_date_february_common_year() {
        let p = FiscalPeriod::try_new(2026, Some(2), Some(2)).unwrap();
        assert_eq!(p.end_date(), NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
    }

    #[test]
    fn test_end_date_february_leap_year() {
        let p = FiscalPeriod::try_new(2024, Some(2), Some(2)).unwrap();
        assert_eq!(p.end_date(), NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());
    }

    #[test]
    fn test_end_date_december() {
        let p = FiscalPeriod::try_new(2026, Some(12), Some(12)).unwrap();
        assert_eq!(p.end_date(), NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
    }

    #[test]
    fn test_end_date_cross_month_range() {
        let p = FiscalPeriod::try_new(2026, Some(11), Some(12)).unwrap();
        assert_eq!(p.end_date(), NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
    }

    #[test]
    fn test_label_single_month() {
        let p = FiscalPeriod::try_new(2026, Some(7), Some(7)).unwrap();
        assert_eq!(p.label(), "2026-07");
    }

    #[test]
    fn test_label_multi_month() {
        let p = FiscalPeriod::try_new(2026, Some(1), Some(12)).unwrap();
        assert_eq!(p.label(), "2026-01~2026-12");
    }
}
