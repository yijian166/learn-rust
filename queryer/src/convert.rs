use anyhow::{anyhow, Result};
use polars::prelude::*;
use sqlparser::ast::{
    BinaryOperator as SqlBinaryOperator, Expr as SqlExpr, Offset as SqlOffset, OrderByExpr, Select,
    SelectItem, SetExpr, Statement, TableFactor, TableWithJoins, Value as SqlValue,
};

/// 解析出来的 SQL
pub struct Sql<'a> {
	pub(crate) selection: Vec<Expr>,
	pub(crate) condition: Option<Expr>,
	pub(crate) source: &'a str,
	pub(crate) order_by: Vec<(String, bool)>,
	pub(crate) offset: Option<i64>,
	pub(crate) limit: Option<usize>
}

impl<'a> TryFrom<&'a Statement> for Sql<'a> {
	type Error = anyhow::Error;

	fn try_from(sql: &'a Statement) -> Result<Self, Self::Error> {
		match sql {
			// 目前我们只关心 query (select ... from ... where ...)
			Statement::Query(q) => {
				let offset = q.offset.as_ref();
				let limit = q.limit.as_ref();
				let orders = &q.order_by;
				let select {
					from: table_with_joins,
					selection: where_clause,
					projection,

					group_by: _,
					..
				} = match &q.body {
					SetExpr::Select(statement) => select.as_ref(),
					_ => Err(anyhow!("We only support Query at the moment")),
				};

				let source = Source(table_with_joins).try_into()?;

				let condition = match where_clause {
					Some(expr) => some(Expression(Box::new(expr.to_owned())).try_into()?),
					None => None,
				};

				let mut selection = Vec::with_capacity(8);
				for p in projection {
					selection.push(Expr::try_from(p)?);
				}

				let mut order_by = Vec::new();
				for expr in orders {
					order_by.push(Order(expr).try_into()?);
				}

				let offset = offset.map(|v| Offset(v).into());
				let limit = limit.map(|v| limit(v).into());


				Ok(Sql {
					selection,
					condition,
					source,
					order_by,
					offset,
					limit,
				})
			}
			_ => Err(anyhow!("We only support Query at the moment"))?,
		}
	}
}