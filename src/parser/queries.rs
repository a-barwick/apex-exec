use super::Parser;
use crate::{
    ast::{
        Expression, FieldPath, NullsOrder, SoqlAggregateFunction, SoqlComparisonOperator,
        SoqlCondition, SoqlDateLiteral, SoqlDateLiteralKind, SoqlInValues, SoqlLogicalOperator,
        SoqlOrderBy, SoqlQuery, SoqlSelectItem, SoqlValue, SortDirection, SoslQuery, SoslReturning,
        SoslScope,
    },
    diagnostic::Diagnostic,
    token::TokenKind,
};

impl Parser {
    pub(super) fn parse_query_expression(&mut self) -> Result<Expression, Diagnostic> {
        let start = self.expect_simple(TokenKind::LeftBracket, "expected `[`")?;
        if self.check_keyword("select") {
            let mut query = self.parse_soql_body()?;
            let end =
                self.expect_simple(TokenKind::RightBracket, "expected `]` after SOQL query")?;
            query.span = start.span.merge(end.span);
            Ok(Expression::Soql(Box::new(query)))
        } else if self.check_keyword("find") {
            let mut query = self.parse_sosl_body()?;
            let end =
                self.expect_simple(TokenKind::RightBracket, "expected `]` after SOSL query")?;
            query.span = start.span.merge(end.span);
            Ok(Expression::Sosl(Box::new(query)))
        } else {
            Err(Diagnostic::new(
                "expected `SELECT` or `FIND` after `[`",
                self.current().span,
            ))
        }
    }

    pub(super) fn parse_soql_body(&mut self) -> Result<SoqlQuery, Diagnostic> {
        let start = self.expect_keyword("select", "expected `SELECT`")?;
        let select = self.parse_select_list()?;
        self.expect_keyword("from", "expected `FROM` after SOQL select list")?;
        let from = self.expect_identifier("expected an SObject type after `FROM`")?;
        let where_clause = self.parse_optional_condition("where")?;
        let group_by = self.parse_optional_group_by()?;
        let having = self.parse_optional_condition("having")?;
        let order_by = self.parse_optional_order_by()?;
        let limit = self.parse_optional_value_clause("limit")?;
        let offset = self.parse_optional_value_clause("offset")?;
        let end = offset
            .as_ref()
            .or(limit.as_ref())
            .map_or(from.span, SoqlValue::span);
        Ok(SoqlQuery {
            select,
            from,
            where_clause,
            group_by,
            having,
            order_by,
            limit,
            offset,
            span: start.span.merge(end),
        })
    }

    fn parse_select_list(&mut self) -> Result<Vec<SoqlSelectItem>, Diagnostic> {
        let mut select = Vec::new();
        loop {
            select.push(self.parse_select_item()?);
            if !self.check(&TokenKind::Comma) {
                return Ok(select);
            }
            self.advance();
        }
    }

    fn parse_optional_condition(
        &mut self,
        keyword: &str,
    ) -> Result<Option<SoqlCondition>, Diagnostic> {
        if !self.check_keyword(keyword) {
            return Ok(None);
        }
        self.advance();
        self.parse_soql_or().map(Some)
    }

    fn parse_optional_group_by(&mut self) -> Result<Vec<FieldPath>, Diagnostic> {
        if !self.check_keyword("group") {
            return Ok(Vec::new());
        }
        self.advance();
        self.expect_keyword("by", "expected `BY` after `GROUP`")?;
        self.parse_field_path_list()
    }

    fn parse_select_item(&mut self) -> Result<SoqlSelectItem, Diagnostic> {
        if self.check_keyword("typeof") {
            return Err(Diagnostic::new(
                "`TYPEOF` polymorphic SOQL is not supported by the active compatibility profile",
                self.current().span,
            ));
        }
        if self.check(&TokenKind::LeftParen) {
            return self.parse_child_select_item();
        }
        if let Some(function) = self.current_aggregate_function() {
            return self.parse_aggregate_select_item(function);
        }
        Ok(SoqlSelectItem::Field(self.parse_field_path()?))
    }

    fn parse_child_select_item(&mut self) -> Result<SoqlSelectItem, Diagnostic> {
        let start = self.advance();
        if !self.check_keyword("select") {
            return Err(Diagnostic::new(
                "expected `SELECT` after `(` in SOQL select list",
                self.current().span,
            ));
        }
        let mut query = self.parse_soql_body()?;
        let end = self.expect_simple(
            TokenKind::RightParen,
            "expected `)` after child SOQL subquery",
        )?;
        let span = start.span.merge(end.span);
        query.span = span;
        Ok(SoqlSelectItem::Subquery {
            query: Box::new(query),
            span,
        })
    }

    fn parse_aggregate_select_item(
        &mut self,
        function: SoqlAggregateFunction,
    ) -> Result<SoqlSelectItem, Diagnostic> {
        let start = self.advance();
        self.advance();
        let field = self.parse_optional_aggregate_field()?;
        let close = self.expect_simple(
            TokenKind::RightParen,
            "expected `)` after aggregate argument",
        )?;
        let alias = if matches!(self.current().kind, TokenKind::Identifier(_))
            && !self.is_query_clause_keyword()
        {
            Some(self.expect_identifier("expected aggregate alias")?)
        } else {
            None
        };
        let end = alias.as_ref().map_or(close.span, |alias| alias.span);
        Ok(SoqlSelectItem::Aggregate {
            function,
            field,
            alias,
            span: start.span.merge(end),
        })
    }

    fn parse_field_path(&mut self) -> Result<FieldPath, Diagnostic> {
        let first = self.expect_identifier("expected a field name")?;
        let mut span = first.span;
        let mut segments = vec![first];
        while self.check(&TokenKind::Dot) {
            self.advance();
            let segment = self.expect_identifier("expected a field name after `.`")?;
            span = span.merge(segment.span);
            segments.push(segment);
        }
        Ok(FieldPath { segments, span })
    }

    fn parse_field_path_list(&mut self) -> Result<Vec<FieldPath>, Diagnostic> {
        let mut fields = Vec::new();
        loop {
            fields.push(self.parse_field_path()?);
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance();
        }
        Ok(fields)
    }

    fn parse_soql_or(&mut self) -> Result<SoqlCondition, Diagnostic> {
        let mut condition = self.parse_soql_and()?;
        while self.check_keyword("or") {
            self.advance();
            let right = self.parse_soql_and()?;
            let span = condition.span().merge(right.span());
            condition = SoqlCondition::Logical {
                left: Box::new(condition),
                operator: SoqlLogicalOperator::Or,
                right: Box::new(right),
                span,
            };
        }
        Ok(condition)
    }

    fn parse_soql_and(&mut self) -> Result<SoqlCondition, Diagnostic> {
        let mut condition = self.parse_soql_not()?;
        while self.check_keyword("and") {
            self.advance();
            let right = self.parse_soql_not()?;
            let span = condition.span().merge(right.span());
            condition = SoqlCondition::Logical {
                left: Box::new(condition),
                operator: SoqlLogicalOperator::And,
                right: Box::new(right),
                span,
            };
        }
        Ok(condition)
    }

    fn parse_soql_not(&mut self) -> Result<SoqlCondition, Diagnostic> {
        if self.check_keyword("not") {
            let start = self.advance();
            let condition = self.parse_soql_not()?;
            let span = start.span.merge(condition.span());
            return Ok(SoqlCondition::Not {
                condition: Box::new(condition),
                span,
            });
        }
        if self.check(&TokenKind::LeftParen) {
            self.advance();
            let condition = self.parse_soql_or()?;
            self.expect_simple(TokenKind::RightParen, "expected `)` after SOQL condition")?;
            return Ok(condition);
        }
        self.parse_soql_predicate()
    }

    fn parse_soql_predicate(&mut self) -> Result<SoqlCondition, Diagnostic> {
        if let Some(function) = self.current_aggregate_function() {
            return self.parse_aggregate_predicate(function);
        }
        let field = self.parse_field_path()?;
        let start = field.span;
        let negated_in = if self.check_keyword("not") && self.peek_keyword(1, "in") {
            self.advance();
            self.advance();
            Some(true)
        } else if self.check_keyword("in") {
            self.advance();
            Some(false)
        } else {
            let operator = self.parse_soql_comparison_operator()?;
            let right = self.parse_soql_value()?;
            let span = start.merge(right.span());
            return Ok(SoqlCondition::Comparison {
                left: field,
                operator,
                right,
                span,
            });
        };
        self.parse_in_predicate(field, start, negated_in.expect("IN branch is present"))
    }

    fn parse_in_predicate(
        &mut self,
        field: FieldPath,
        start: crate::span::Span,
        negated: bool,
    ) -> Result<SoqlCondition, Diagnostic> {
        let values = if self.check(&TokenKind::Colon) {
            self.advance();
            SoqlInValues::Bind(Box::new(self.parse_expression()?))
        } else {
            self.expect_simple(TokenKind::LeftParen, "expected `(` or bind after `IN`")?;
            let mut values = Vec::new();
            if !self.check(&TokenKind::RightParen) {
                loop {
                    values.push(self.parse_soql_value()?);
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance();
                }
            }
            self.expect_simple(TokenKind::RightParen, "expected `)` after `IN` values")?;
            SoqlInValues::Values(values)
        };
        let end = match &values {
            SoqlInValues::Values(values) => values.last().map_or(start, SoqlValue::span),
            SoqlInValues::Bind(expression) => expression.span(),
        };
        Ok(SoqlCondition::In {
            field,
            negated,
            values,
            span: start.merge(end),
        })
    }

    fn parse_aggregate_predicate(
        &mut self,
        function: SoqlAggregateFunction,
    ) -> Result<SoqlCondition, Diagnostic> {
        let start = self.advance();
        self.advance();
        let field = self.parse_optional_aggregate_field()?;
        self.expect_simple(
            TokenKind::RightParen,
            "expected `)` after HAVING aggregate argument",
        )?;
        let operator = self.parse_soql_comparison_operator()?;
        let right = self.parse_soql_value()?;
        Ok(SoqlCondition::AggregateComparison {
            function,
            field,
            operator,
            span: start.span.merge(right.span()),
            right,
        })
    }

    fn parse_optional_aggregate_field(&mut self) -> Result<Option<FieldPath>, Diagnostic> {
        if self.check(&TokenKind::RightParen) {
            Ok(None)
        } else {
            self.parse_field_path().map(Some)
        }
    }

    fn current_aggregate_function(&self) -> Option<SoqlAggregateFunction> {
        let TokenKind::Identifier(name) = &self.current().kind else {
            return None;
        };
        if !matches!(self.peek(1).kind, TokenKind::LeftParen) {
            return None;
        }
        match name.to_ascii_lowercase().as_str() {
            "count" => Some(SoqlAggregateFunction::Count),
            "sum" => Some(SoqlAggregateFunction::Sum),
            "min" => Some(SoqlAggregateFunction::Min),
            "max" => Some(SoqlAggregateFunction::Max),
            _ => None,
        }
    }

    fn parse_soql_comparison_operator(&mut self) -> Result<SoqlComparisonOperator, Diagnostic> {
        let operator = if self.check(&TokenKind::Equal) {
            SoqlComparisonOperator::Equal
        } else if self.check(&TokenKind::BangEqual) {
            SoqlComparisonOperator::NotEqual
        } else if self.check(&TokenKind::Less) {
            SoqlComparisonOperator::Less
        } else if self.check(&TokenKind::LessEqual) {
            SoqlComparisonOperator::LessEqual
        } else if self.check(&TokenKind::Greater) {
            SoqlComparisonOperator::Greater
        } else if self.check(&TokenKind::GreaterEqual) {
            SoqlComparisonOperator::GreaterEqual
        } else if self.check_keyword("like") {
            SoqlComparisonOperator::Like
        } else {
            return Err(Diagnostic::new(
                "expected a SOQL comparison operator",
                self.current().span,
            ));
        };
        self.advance();
        Ok(operator)
    }

    fn parse_soql_value(&mut self) -> Result<SoqlValue, Diagnostic> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::StringLiteral(value) => {
                self.advance();
                Ok(SoqlValue::String(value, token.span))
            }
            TokenKind::BooleanLiteral(value) => {
                self.advance();
                Ok(SoqlValue::Boolean(value, token.span))
            }
            TokenKind::IntegerLiteral(value) => {
                self.advance();
                Ok(SoqlValue::Integer(value, token.span))
            }
            TokenKind::Identifier(name) => {
                let Some(kind) = soql_date_literal_kind(&name) else {
                    return Err(Diagnostic::new(
                        "expected a SOQL literal or bind expression",
                        token.span,
                    ));
                };
                self.advance();
                let (amount, span) = if matches!(
                    kind,
                    SoqlDateLiteralKind::LastNDays | SoqlDateLiteralKind::NextNDays
                ) {
                    self.expect_simple(
                        TokenKind::Colon,
                        "expected `:` in relative SOQL date literal",
                    )?;
                    let amount = self.current().clone();
                    let TokenKind::IntegerLiteral(value) = amount.kind else {
                        return Err(Diagnostic::new(
                            "expected a non-negative Integer in relative SOQL date literal",
                            amount.span,
                        ));
                    };
                    self.advance();
                    if value < 0 {
                        return Err(Diagnostic::new(
                            "relative SOQL date literal amount cannot be negative",
                            amount.span,
                        ));
                    }
                    (Some(value), token.span.merge(amount.span))
                } else {
                    (None, token.span)
                };
                Ok(SoqlValue::DateLiteral(SoqlDateLiteral {
                    kind,
                    amount,
                    span,
                }))
            }
            TokenKind::Null => {
                self.advance();
                Ok(SoqlValue::Null(token.span))
            }
            TokenKind::Colon => {
                self.advance();
                let expression = self.parse_expression()?;
                let span = token.span.merge(expression.span());
                Ok(SoqlValue::Bind(Box::new(expression), span))
            }
            _ => Err(Diagnostic::new(
                "expected a SOQL literal or bind expression",
                token.span,
            )),
        }
    }

    fn parse_optional_order_by(&mut self) -> Result<Vec<SoqlOrderBy>, Diagnostic> {
        if !self.check_keyword("order") {
            return Ok(Vec::new());
        }
        self.advance();
        self.expect_keyword("by", "expected `BY` after `ORDER`")?;
        let mut ordering = Vec::new();
        loop {
            let field = self.parse_field_path()?;
            let direction = if self.check_keyword("desc") {
                self.advance();
                SortDirection::Descending
            } else {
                if self.check_keyword("asc") {
                    self.advance();
                }
                SortDirection::Ascending
            };
            let nulls = if self.check_keyword("nulls") {
                self.advance();
                if self.check_keyword("first") {
                    self.advance();
                    Some(NullsOrder::First)
                } else if self.check_keyword("last") {
                    self.advance();
                    Some(NullsOrder::Last)
                } else {
                    return Err(Diagnostic::new(
                        "expected `FIRST` or `LAST` after `NULLS`",
                        self.current().span,
                    ));
                }
            } else {
                None
            };
            let span = field.span;
            ordering.push(SoqlOrderBy {
                field,
                direction,
                nulls,
                span,
            });
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance();
        }
        Ok(ordering)
    }

    fn parse_optional_value_clause(
        &mut self,
        keyword: &str,
    ) -> Result<Option<SoqlValue>, Diagnostic> {
        if self.check_keyword(keyword) {
            self.advance();
            Ok(Some(self.parse_soql_value()?))
        } else {
            Ok(None)
        }
    }

    fn parse_sosl_body(&mut self) -> Result<SoslQuery, Diagnostic> {
        let start = self.expect_keyword("find", "expected `FIND`")?;
        let search = self.parse_soql_value()?;
        self.expect_keyword("in", "expected `IN` after SOSL search term")?;
        let scope = if self.check_keyword("all") {
            self.advance();
            SoslScope::AllFields
        } else if self.check_keyword("name") {
            self.advance();
            SoslScope::NameFields
        } else {
            return Err(Diagnostic::new(
                "expected `ALL` or `NAME` SOSL field scope",
                self.current().span,
            ));
        };
        self.expect_keyword("fields", "expected `FIELDS` after SOSL scope")?;
        self.expect_keyword("returning", "expected `RETURNING` in SOSL query")?;
        let mut returning = Vec::new();
        loop {
            returning.push(self.parse_sosl_returning()?);
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance();
        }
        let end = returning.last().map_or(search.span(), |clause| clause.span);
        Ok(SoslQuery {
            search,
            scope,
            returning,
            span: start.span.merge(end),
        })
    }

    fn parse_sosl_returning(&mut self) -> Result<SoslReturning, Diagnostic> {
        let object = self.expect_identifier("expected an SObject type in `RETURNING`")?;
        self.expect_simple(
            TokenKind::LeftParen,
            "expected `(` after SOSL returning object",
        )?;
        let fields = self.parse_field_path_list()?;
        let where_clause = if self.check_keyword("where") {
            self.advance();
            Some(self.parse_soql_or()?)
        } else {
            None
        };
        let order_by = self.parse_optional_order_by()?;
        let limit = self.parse_optional_value_clause("limit")?;
        let end = self.expect_simple(
            TokenKind::RightParen,
            "expected `)` after SOSL returning clause",
        )?;
        Ok(SoslReturning {
            object: object.clone(),
            fields,
            where_clause,
            order_by,
            limit,
            span: object.span.merge(end.span),
        })
    }

    fn peek_keyword(&self, offset: usize, expected: &str) -> bool {
        matches!(
            &self.peek(offset).kind,
            TokenKind::Identifier(spelling) if spelling.eq_ignore_ascii_case(expected)
        )
    }

    fn is_query_clause_keyword(&self) -> bool {
        [
            "from", "where", "group", "having", "order", "limit", "offset", "asc", "desc", "nulls",
            "and", "or",
        ]
        .iter()
        .any(|keyword| self.check_keyword(keyword))
    }
}

fn soql_date_literal_kind(name: &str) -> Option<SoqlDateLiteralKind> {
    match name.to_ascii_lowercase().as_str() {
        "yesterday" => Some(SoqlDateLiteralKind::Yesterday),
        "today" => Some(SoqlDateLiteralKind::Today),
        "tomorrow" => Some(SoqlDateLiteralKind::Tomorrow),
        "last_n_days" => Some(SoqlDateLiteralKind::LastNDays),
        "next_n_days" => Some(SoqlDateLiteralKind::NextNDays),
        "this_week" => Some(SoqlDateLiteralKind::ThisWeek),
        "last_week" => Some(SoqlDateLiteralKind::LastWeek),
        "next_week" => Some(SoqlDateLiteralKind::NextWeek),
        "this_month" => Some(SoqlDateLiteralKind::ThisMonth),
        "last_month" => Some(SoqlDateLiteralKind::LastMonth),
        "next_month" => Some(SoqlDateLiteralKind::NextMonth),
        "this_year" => Some(SoqlDateLiteralKind::ThisYear),
        "last_year" => Some(SoqlDateLiteralKind::LastYear),
        "next_year" => Some(SoqlDateLiteralKind::NextYear),
        _ => None,
    }
}
