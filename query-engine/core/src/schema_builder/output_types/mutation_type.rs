use super::*;
use crate::{write, QueryGraph};
use prisma_models::{dml, PrismaValue};

/// Builds the root `Mutation` type.
pub(crate) fn build(ctx: &mut BuilderContext) -> (OutputType, ObjectTypeStrongRef) {
    let non_embedded_models = ctx.internal_data_model.non_embedded_models();
    let mut fields: Vec<Field> = non_embedded_models
        .into_iter()
        .map(|model| {
            let mut vec = vec![create_item_field(ctx, &model)];

            append_opt(&mut vec, delete_item_field(ctx, &model));
            append_opt(&mut vec, update_item_field(ctx, &model));
            append_opt(&mut vec, upsert_item_field(ctx, &model));

            vec.push(update_many_field(ctx, &model));
            vec.push(delete_many_field(ctx, &model));

            vec
        })
        .flatten()
        .collect();

    if ctx.enable_raw_queries {
        fields.push(create_execute_raw_field());
        fields.push(create_query_raw_field());
    }

    let strong_ref = Arc::new(object_type("Mutation", fields, None));

    (OutputType::Object(Arc::downgrade(&strong_ref)), strong_ref)
}

fn create_execute_raw_field() -> Field {
    field(
        "executeRaw",
        vec![
            argument("query", InputType::string(), None),
            argument(
                "parameters",
                InputType::opt(InputType::json_list()),
                Some(dml::DefaultValue::Single(PrismaValue::String("[]".into()))),
            ),
        ],
        OutputType::json(),
        None,
    )
}

fn create_query_raw_field() -> Field {
    field(
        "queryRaw",
        vec![
            argument("query", InputType::string(), None),
            argument(
                "parameters",
                InputType::opt(InputType::json_list()),
                Some(dml::DefaultValue::Single(PrismaValue::String("[]".into()))),
            ),
        ],
        OutputType::json(),
        None,
    )
}

/// Builds a create mutation field (e.g. createUser) for given model.
fn create_item_field(ctx: &mut BuilderContext, model: &ModelRef) -> Field {
    let args = arguments::create_arguments(ctx, model).unwrap_or_else(|| vec![]);
    let field_name = ctx.pluralize_internal(format!("create{}", model.name), format!("createOne{}", model.name));

    field(
        field_name,
        args,
        OutputType::object(output_objects::map_model_object_type(ctx, &model)),
        Some(SchemaQueryBuilder::ModelQueryBuilder(ModelQueryBuilder::new(
            model.clone(),
            QueryTag::CreateOne,
            Box::new(|model, parsed_field| {
                let mut graph = QueryGraph::new();

                write::create_record(&mut graph, model, parsed_field)?;
                Ok(graph)
            }),
        ))),
    )
}

/// Builds a delete mutation field (e.g. deleteUser) for given model.
fn delete_item_field(ctx: &mut BuilderContext, model: &ModelRef) -> Option<Field> {
    arguments::delete_arguments(ctx, model).map(|args| {
        let field_name = ctx.pluralize_internal(format!("delete{}", model.name), format!("deleteOne{}", model.name));

        field(
            field_name,
            args,
            OutputType::opt(OutputType::object(output_objects::map_model_object_type(ctx, &model))),
            Some(SchemaQueryBuilder::ModelQueryBuilder(ModelQueryBuilder::new(
                model.clone(),
                QueryTag::DeleteOne,
                Box::new(|model, parsed_field| {
                    let mut graph = QueryGraph::new();

                    write::delete_record(&mut graph, model, parsed_field)?;
                    Ok(graph)
                }),
            ))),
        )
    })
}

/// Builds a delete many mutation field (e.g. deleteManyUsers) for given model.
fn delete_many_field(ctx: &mut BuilderContext, model: &ModelRef) -> Field {
    let arguments = arguments::delete_many_arguments(ctx, model);
    let field_name = ctx.pluralize_internal(
        format!("deleteMany{}", pluralize(&model.name)),
        format!("deleteMany{}", model.name),
    );

    field(
        field_name,
        arguments,
        OutputType::object(output_objects::batch_payload_object_type(ctx)),
        Some(SchemaQueryBuilder::ModelQueryBuilder(ModelQueryBuilder::new(
            model.clone(),
            QueryTag::DeleteMany,
            Box::new(|model, parsed_field| {
                let mut graph = QueryGraph::new();

                write::delete_many_records(&mut graph, model, parsed_field)?;
                Ok(graph)
            }),
        ))),
    )
}

/// Builds an update mutation field (e.g. updateUser) for given model.
fn update_item_field(ctx: &mut BuilderContext, model: &ModelRef) -> Option<Field> {
    arguments::update_arguments(ctx, model).map(|args| {
        let field_name = ctx.pluralize_internal(format!("update{}", model.name), format!("updateOne{}", model.name));

        field(
            field_name,
            args,
            OutputType::opt(OutputType::object(output_objects::map_model_object_type(ctx, &model))),
            Some(SchemaQueryBuilder::ModelQueryBuilder(ModelQueryBuilder::new(
                model.clone(),
                QueryTag::UpdateOne,
                Box::new(|model, parsed_field| {
                    let mut graph = QueryGraph::new();

                    write::update_record(&mut graph, model, parsed_field)?;
                    Ok(graph)
                }),
            ))),
        )
    })
}

/// Builds an update many mutation field (e.g. updateManyUsers) for given model.
fn update_many_field(ctx: &mut BuilderContext, model: &ModelRef) -> Field {
    let arguments = arguments::update_many_arguments(ctx, model);
    let field_name = ctx.pluralize_internal(
        format!("updateMany{}", pluralize(model.name.as_str())),
        format!("updateMany{}", model.name),
    );

    field(
        field_name,
        arguments,
        OutputType::object(output_objects::batch_payload_object_type(ctx)),
        Some(SchemaQueryBuilder::ModelQueryBuilder(ModelQueryBuilder::new(
            model.clone(),
            QueryTag::UpdateMany,
            Box::new(|model, parsed_field| {
                let mut graph = QueryGraph::new();

                write::update_many_records(&mut graph, model, parsed_field)?;
                Ok(graph)
            }),
        ))),
    )
}

/// Builds an upsert mutation field (e.g. upsertUser) for given model.
fn upsert_item_field(ctx: &mut BuilderContext, model: &ModelRef) -> Option<Field> {
    arguments::upsert_arguments(ctx, model).map(|args| {
        let field_name = ctx.pluralize_internal(format!("upsert{}", model.name), format!("upsertOne{}", model.name));

        field(
            field_name,
            args,
            OutputType::object(output_objects::map_model_object_type(ctx, &model)),
            Some(SchemaQueryBuilder::ModelQueryBuilder(ModelQueryBuilder::new(
                model.clone(),
                QueryTag::UpsertOne,
                Box::new(|model, parsed_field| {
                    let mut graph = QueryGraph::new();

                    write::upsert_record(&mut graph, model, parsed_field)?;
                    Ok(graph)
                }),
            ))),
        )
    })
}