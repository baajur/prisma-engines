use super::{common, IteratorJoin, Quoted, QuotedWithSchema, SqlRenderer};
use crate::{
    database_info::DatabaseInfo,
    flavour::MssqlFlavour,
    sql_migration::{
        AddColumn, AddForeignKey, AlterColumn, AlterEnum, AlterIndex, AlterTable, CreateEnum, CreateIndex, DropColumn,
        DropEnum, DropForeignKey, DropIndex, TableChange,
    },
    sql_schema_differ::SqlSchemaDiffer,
};
use prisma_value::PrismaValue;
use sql_schema_describer::{
    walkers::{ColumnWalker, TableWalker},
    ColumnTypeFamily, DefaultValue, ForeignKey, IndexType, SqlSchema,
};
use std::{borrow::Cow, fmt::Write};

impl SqlRenderer for MssqlFlavour {
    fn quote<'a>(&self, name: &'a str) -> Quoted<&'a str> {
        Quoted::mssql_ident(name)
    }

    fn quote_with_schema<'a, 'b>(&'a self, name: &'b str) -> QuotedWithSchema<'a, &'b str> {
        QuotedWithSchema {
            schema_name: self.schema_name(),
            name: self.quote(name),
        }
    }

    fn render_alter_table(&self, alter_table: &AlterTable, differ: &SqlSchemaDiffer<'_>) -> Vec<String> {
        let AlterTable { table, changes } = alter_table;

        let mut lines = Vec::new();

        for change in changes {
            match change {
                TableChange::DropPrimaryKey { constraint_name } => {
                    let constraint = constraint_name.as_ref().unwrap();
                    lines.push(format!("DROP CONSTRAINT {}", self.quote(constraint)));
                }
                TableChange::AddPrimaryKey { columns } => {
                    let columns = columns.iter().map(|colname| self.quote(colname)).join(", ");
                    lines.push(format!("ADD PRIMARY KEY ({})", columns));
                }
                TableChange::AddColumn(AddColumn { column }) => {
                    let column = ColumnWalker {
                        table,
                        schema: differ.next,
                        column,
                    };
                    let col_sql = self.render_column(column);
                    lines.push(format!("ADD COLUMN {}", col_sql));
                }
                TableChange::DropColumn(DropColumn { name }) => {
                    let name = self.quote(&name);
                    lines.push(format!("DROP COLUMN {}", name));
                }
                TableChange::AlterColumn(AlterColumn { .. }) => todo!("We must handle altering columns in MSSQL"),
            };
        }

        if lines.is_empty() {
            return Vec::new();
        }

        vec![format!(
            "ALTER TABLE {} {}",
            self.quote_with_schema(&table.name),
            lines.join(",\n")
        )]
    }

    fn render_alter_enum(&self, _: &AlterEnum, _: &SqlSchemaDiffer<'_>) -> anyhow::Result<Vec<String>> {
        unreachable!("render_alter_enum on Microsoft SQL Server")
    }

    fn render_column(&self, column: ColumnWalker<'_>) -> String {
        let column_name = self.quote(column.name());

        let r#type = match &column.column_type().family {
            ColumnTypeFamily::Boolean => "bit",
            ColumnTypeFamily::DateTime => "datetime2",
            ColumnTypeFamily::Float => "decimal(32,16)",
            ColumnTypeFamily::Int => "int",
            ColumnTypeFamily::String | ColumnTypeFamily::Json => "nvarchar(1000)",
            x => unimplemented!("{:?} not handled yet", x),
        };

        let nullability = common::render_nullability(&column);

        let default = column
            .default()
            .filter(|default| !matches!(default, DefaultValue::DBGENERATED(_)))
            .map(|default| format!("DEFAULT {}", self.render_default(default, &column.column.tpe.family)))
            .unwrap_or_else(String::new);

        if column.is_autoincrement() {
            format!("{} int IDENTITY(1,1)", column_name)
        } else {
            format!("{} {} {} {}", column_name, r#type, nullability, default)
        }
    }

    fn render_references(&self, table: &str, foreign_key: &ForeignKey) -> String {
        let cols = foreign_key.referenced_columns.iter().map(Quoted::mssql_ident).join(",");
        let is_self_relation = table == foreign_key.referenced_table;

        let (on_delete, on_update) = if is_self_relation {
            ("ON DELETE NO ACTION", "ON UPDATE NO ACTION")
        } else {
            let on_delete = common::render_on_delete(&foreign_key.on_delete_action).into();
            let on_update = common::render_on_update(&foreign_key.on_update_action).into();

            (on_delete, on_update)
        };

        format!(
            " REFERENCES {}({}) {} {}",
            self.quote_with_schema(&foreign_key.referenced_table),
            cols,
            on_delete,
            on_update
        )
    }

    fn render_default<'a>(&self, default: &'a DefaultValue, family: &ColumnTypeFamily) -> Cow<'a, str> {
        match (default, family) {
            (DefaultValue::DBGENERATED(val), _) => val.as_str().into(),
            (DefaultValue::VALUE(PrismaValue::String(val)), ColumnTypeFamily::String)
            | (DefaultValue::VALUE(PrismaValue::Enum(val)), ColumnTypeFamily::Enum(_)) => {
                format!("'{}'", escape_string_literal(&val)).into()
            }
            (DefaultValue::NOW, ColumnTypeFamily::DateTime) => "CURRENT_TIMESTAMP".into(),
            (DefaultValue::NOW, _) => unreachable!("NOW default on non-datetime column"),
            (DefaultValue::VALUE(val), ColumnTypeFamily::DateTime) => format!("'{}'", val).into(),
            (DefaultValue::VALUE(PrismaValue::String(val)), ColumnTypeFamily::Json) => format!("'{}'", val).into(),
            (DefaultValue::VALUE(PrismaValue::Boolean(val)), ColumnTypeFamily::Boolean) => {
                Cow::from(if *val { "1" } else { "0" })
            }
            (DefaultValue::VALUE(val), _) => val.to_string().into(),
            (DefaultValue::SEQUENCE(_), _) => "".into(),
        }
    }

    fn render_alter_index(
        &self,
        alter_index: &AlterIndex,
        _database_info: &DatabaseInfo,
        _current_schema: &SqlSchema,
    ) -> anyhow::Result<Vec<String>> {
        let AlterIndex {
            table,
            index_name,
            index_new_name,
        } = alter_index;

        let index_with_table = Quoted::Single(format!("{}.{}.{}", self.schema_name(), table, index_name));

        Ok(vec![format!(
            "EXEC SP_RENAME N{index_with_table}, N{index_new_name}, N'INDEX'",
            index_with_table = Quoted::Single(index_with_table),
            index_new_name = Quoted::Single(index_new_name),
        )])
    }

    fn render_create_enum(&self, _: &CreateEnum) -> Vec<String> {
        unreachable!("render_create_enum on Microsoft SQL Server")
    }

    fn render_create_index(&self, create_index: &CreateIndex) -> String {
        let CreateIndex {
            table,
            index,
            caused_by_create_table: _,
            contains_nullable_columns,
        } = create_index;

        let index_type = match index.tpe {
            IndexType::Unique => "UNIQUE ",
            IndexType::Normal => "",
        };

        let index_name = index.name.replace('.', "_");
        let index_name = self.quote(&index_name);
        let table_reference = self.quote_with_schema(&table).to_string();

        let condition = match index.tpe {
            IndexType::Unique if *contains_nullable_columns => {
                let columns = index
                    .columns
                    .iter()
                    .map(|c| self.quote(c))
                    .map(|c| format!("{} IS NOT NULL", c));

                Cow::from(format!(" WHERE {}", columns.join(" AND ")))
            }
            _ => Cow::from(""),
        };

        let columns = index.columns.iter().map(|c| self.quote(c));

        format!(
            "CREATE {index_type}INDEX {index_name} ON {table_reference}({columns}){condition}",
            index_type = index_type,
            index_name = index_name,
            table_reference = table_reference,
            columns = columns.join(", "),
            condition = condition,
        )
    }

    fn render_create_table(&self, table: &TableWalker<'_>) -> anyhow::Result<String> {
        let columns: String = table.columns().map(|column| self.render_column(column)).join(",\n");

        let primary_columns = table.table.primary_key_columns();

        let primary_key = if !primary_columns.is_empty() {
            let index_name = format!("PK_{}_{}", table.table.name, primary_columns.iter().join("_"));
            let column_names = primary_columns.iter().map(|col| self.quote(&col)).join(",");

            format!(",\nCONSTRAINT {} PRIMARY KEY ({})", index_name, column_names)
        } else {
            String::new()
        };

        // We only render unique constraints here if the mapped columns can't be
        // null.
        let constraints = table
            .indexes()
            .filter(|index| index.index.is_unique() && !index.has_nullable_columns())
            .collect::<Vec<_>>();

        let constraints = if !constraints.is_empty() {
            let constraints = constraints
                .iter()
                .map(|index| {
                    let name = index.index.name.replace('.', "_");
                    let columns = index.index.columns.iter().map(|col| self.quote(&col));

                    format!("CONSTRAINT {} UNIQUE ({})", name, columns.join(","))
                })
                .join(",\n");

            format!(",\n{}", constraints)
        } else {
            String::new()
        };

        Ok(format!(
            "CREATE TABLE {} ({columns}{primary_key}{constraints})",
            table_name = self.quote_with_schema(table.name()),
            columns = columns,
            primary_key = primary_key,
            constraints = constraints,
        ))
    }

    fn render_drop_enum(&self, _drop_enum: &DropEnum) -> Vec<String> {
        unreachable!("render_drop_enum on MSSQL")
    }

    fn render_drop_foreign_key(&self, drop_foreign_key: &DropForeignKey) -> String {
        format!(
            "ALTER TABLE {table} DROP CONSTRAINT {constraint_name}",
            table = self.quote_with_schema(&drop_foreign_key.table),
            constraint_name = Quoted::mssql_ident(&drop_foreign_key.constraint_name),
        )
    }

    fn render_drop_index(&self, drop_index: &DropIndex) -> String {
        format!(
            "DROP INDEX {} ON {}",
            self.quote_with_schema(&drop_index.name),
            self.quote_with_schema(&drop_index.table)
        )
    }

    fn render_redefine_tables(&self, _tables: &[String], _differ: SqlSchemaDiffer<'_>) -> Vec<String> {
        unreachable!("render_redefine_table on MSSQL")
    }

    fn render_rename_table(&self, name: &str, new_name: &str) -> String {
        let with_schema = format!("{}.{}", self.schema_name(), name);

        format!(
            "EXEC SP_RENAME N{}, N{}",
            Quoted::Single(with_schema),
            Quoted::Single(new_name),
        )
    }

    fn render_add_foreign_key(&self, add_foreign_key: &AddForeignKey) -> String {
        let AddForeignKey { foreign_key, table } = add_foreign_key;
        let mut add_constraint = String::with_capacity(120);

        write!(
            add_constraint,
            "ALTER TABLE {table} ADD ",
            table = self.quote_with_schema(table)
        )
        .unwrap();

        if let Some(constraint_name) = foreign_key.constraint_name.as_ref() {
            write!(add_constraint, "CONSTRAINT {} ", self.quote(constraint_name)).unwrap();
        }

        write!(
            add_constraint,
            "FOREIGN KEY ({})",
            foreign_key.columns.iter().map(|col| self.quote(col)).join(", ")
        )
        .unwrap();

        add_constraint.push_str(&self.render_references(&table, &foreign_key));

        add_constraint
    }

    fn render_drop_table(&self, table_name: &str) -> Vec<String> {
        vec![format!("DROP TABLE {}", self.quote_with_schema(&table_name))]
    }
}

fn escape_string_literal(s: &str) -> String {
    s.replace('\'', "''")
}
