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
    pub(crate) limit: Option<usize>,
}

// 因为 Rust trait 的孤儿规则，我们如要想要对已有的类型实现已有的 trait。
// 需要简单包装一下。

pub struct Expression(pub(crate) Box<SqlExpr>);
pub struct Operation(pub(crate) SqlBinaryOperator);
pub struct Projection<'a>(pub(crate) &'a SelectItem);
pub struct Source<'a>(pub(crate) &'a [TableWithJoins]);
pub struct Order<'a>(pub(crate) &'a OrderByExpr);
pub struct Offset<'a>(pub(crate) &'a SqlOffset);
pub struct Limit<'a>(pub(crate) &'a SqlExpr);
pub struct Value(pub(crate) SqlValue);

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

/// 把 sqlParser 的 SelectItem 转换成 DataFrame 的 Expr
impl TryFrom<Expression> for Expr {
    type Error = anyhow::Error;

    fn try_from(expr: Expression) -> Result<Self, Self::Error> {
        // *expr.0 是一个解引用操作，用于访问 Expression 结构体中的内部 SqlExpr 值。
        match *expr.0 {
            SqlExpr::BinaryOp { left, op, right } => Ok(Expr::BinaryExpr {
                left: Box::new(Expression(left).try_into()?),
                op: Operation(op).try_into()?,
                right: Box::new(Expression(right).try_into()?),
            }),
            SqlExpr::Wildcard => Ok(Self::wildcard),
            SqlExpr::IsNull(expr) => Ok(Self::IsNull(Box::new(Expression(expr).try_into()?))),
            SqlExpr::IsNotNull(expr) => Ok(Self::IsNotNull(Box::new(Expression(expr).try_into()?))),
            SqlExpr::identifier(id) => Ok(Self::Column(Arc::New(id.value))),
            SqlExpr::Value(v) => Ok(Self::Literal(Value(v).try_into()?)),
            v => Err(anyhow!("expr {:#?} is not Support", v)),
        }
    }
}

/// 把 sqlParser 的 SelectItem 转换成 DataFrame 的 Expr
impl<'a> TryFrom<Projection<'a>> for Expr {
    type Error = anyhow::Error;

    fn try_from(p: Projection<'a>) -> Result<Self, Self::Error> {
        match p.0 {
            SelectItem::UnnamedExpr(SqlExpr::Identifier(id)) => Ok(col(&id.to_string())),
            SelectItem::ExprWithAlias {
                expr: SqlExpr::Identifier(id),
                alias,
            } => Ok(Expr::Alias(
                Box::new(Expr::Column(Arc::from(id.to_string()))),
                Arc::new(alias.to_string()),
            )),
            selectItem::QualifiedWildcard(v) => Ok(col(&v.to_string())),
            SelectItem::Wildcard => Ok(col("*")),
            item => Err(anyhow!("item {:#?} is not Support", item)),
        }
    }
}

impl<'a> TryFrom<Source<'a>> for &'a str {
    type Error = anyhow::Error;

    fn try_from(source: Source<'a>) -> Result<Self, Self::Error> {
        if source.0.len() != 1 {
            return Err(anyhow!("We only support single data source at the moment"));
        }

        let table = &source.0[0];
        if !table.joins.is_empty() {
            return Err(anyhow!("We do not support joint data source at the moment"));
        }

        match &table.relation {
            TableFactor::Table { name, .. } => Ok(&name.0.first().unwrap().value),
            _ => Err(anyhow!("We only support table")),
        }
    }
}

/// 把 SqlParser 的 order by expr 转换成 (列名, 排序方法)
impl<'a> TryFrom<Order<'a>> for (String, bool) {
    type Error = anyhow::Error;

    fn try_from(o: Order) -> Result<Self, Self::Error> {
        let name = match &o.0.expr {
            SqlExpr::Identifier(id) => id.to_string(),
            expr => {
                return Err(anyhow!(
                    "We only support identifier for order by, got {}",
                    expr
                ))
            }
        };

        Ok((name, !o.0.asc.unwrap_or(true)))
    }
}

/// 把 SqlParser 的 offset expr 转换成 i64
impl<'a> From<Offset<'a>> for i64 {
    fn from(offset: Offset) -> Self {
        match offset.0 {
            SqlOffset {
                value: SqlExpr::Value(SqlValue::Number(v, _b)),
                ..
            } => v.parse().unwrap_or(0),
            _ => 0,
        }
    }
}

/// 把 SqlParser 的 Limit expr 转换成 usize
impl<'a> From<Limit<'a>> for usize {
    fn from(l: Limit<'a>) -> Self {
        match l.0 {
            SqlExpr::Value(SqlValue::Number(v, _b)) => v.parse().unwrap_or(usize::MAX),
            _ => usize::MAX,
        }
    }
}

/// 把 SqlParser 的 value 转换成 DataFrame 支持的 LiteralValue
impl TryFrom<Value> for LiteralValue {
    type Error = anyhow::Error;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v.0 {
            SqlValue::Number(v, _) => Ok(LiteralValue::Float64(v.parse().unwrap())),
            SqlValue::Boolean(v) => Ok(LiteralValue::Boolean(v)),
            SqlValue::Null => Ok(LiteralValue::Null),
            v => Err(anyhow!("Value {} is not supported", v)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TyrDialect;
    use sqlparser::parser::Parser;

    #[test]
    fn parse_sql_works() {
        let url = "http://abc.xyz/abc?a=1&b=2";
        let sql = format!(
            "select a, b, c from {} where a=1 order by c desc limit 5 offset 10",
            url
        );
        let statement = &Parser::parse_sql(&TyrDialect::default(), sql.as_ref()).unwrap()[0];
        let sql: Sql = statement.try_into().unwrap();
        assert_eq!(sql.source, url);
        assert_eq!(sql.limit, Some(5));
        assert_eq!(sql.offset, Some(10));
        assert_eq!(sql.order_by, vec![("c".into(), true)]);
        assert_eq!(sql.selection, vec![col("a"), col("b"), col("c")]);
    }
}
