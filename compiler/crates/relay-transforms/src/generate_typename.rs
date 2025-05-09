/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::sync::Arc;

use common::DirectiveName;
use common::Location;
use common::WithLocation;
use fnv::FnvHashMap;
use graphql_ir::Directive;
use graphql_ir::FragmentDefinition;
use graphql_ir::FragmentSpread;
use graphql_ir::InlineFragment;
use graphql_ir::LinkedField;
use graphql_ir::OperationDefinition;
use graphql_ir::Program;
use graphql_ir::ScalarField;
use graphql_ir::Selection;
use graphql_ir::Transformed;
use graphql_ir::TransformedValue;
use graphql_ir::Transformer;
use intern::string_key::Intern;
use lazy_static::lazy_static;
use schema::SDLSchema;
use schema::Schema;
use schema::Type;

use crate::util::generate_abstract_type_refinement_key;
use crate::util::is_relay_custom_inline_fragment_directive;

lazy_static! {
    pub static ref TYPE_DISCRIMINATOR_DIRECTIVE_NAME: DirectiveName =
        DirectiveName("__TypeDiscriminator".intern());
}

/// Transform to add the `__typename` field to any LinkedField that both a) returns an
/// abstract type and b) does not already directly query `__typename`.
pub fn generate_typename(program: &Program, is_for_codegen: bool) -> Program {
    let mut transform = GenerateTypenameTransform::new(program, is_for_codegen);
    transform
        .transform_program(program)
        .replace_or_else(|| program.clone())
}

type Seen<'a> = FnvHashMap<&'a InlineFragment, Transformed<Selection>>;

struct GenerateTypenameTransform<'s> {
    program: &'s Program,
    seen: Seen<'s>,
    is_for_codegen: bool,
    parent_type: Option<Type>,
}

impl<'s> GenerateTypenameTransform<'s> {
    fn new(program: &'s Program, is_for_codegen: bool) -> Self {
        Self {
            program,
            seen: Default::default(),
            is_for_codegen,
            parent_type: None,
        }
    }
}

impl<'s> Transformer<'s> for GenerateTypenameTransform<'s> {
    const NAME: &'static str = "GenerateTypenameTransform";
    const VISIT_ARGUMENTS: bool = false;
    const VISIT_DIRECTIVES: bool = false;

    fn transform_operation(
        &mut self,
        operation: &'s OperationDefinition,
    ) -> Transformed<OperationDefinition> {
        self.parent_type = Some(operation.type_);
        self.default_transform_operation(operation)
    }

    fn transform_fragment(
        &mut self,
        fragment: &'s FragmentDefinition,
    ) -> Transformed<FragmentDefinition> {
        self.parent_type = Some(fragment.type_condition);
        let schema = &self.program.schema;
        let mut selections = self.transform_selections(&fragment.selections);
        let type_ = fragment.type_condition;
        if !schema.is_extension_type(type_) && type_.is_abstract_type() {
            let mut next_selections = Vec::with_capacity(fragment.selections.len() + 1);
            next_selections.push(generate_abstract_key_field(
                schema,
                type_,
                fragment.name.location,
                self.is_for_codegen,
            ));
            if let TransformedValue::Replace(selections) = selections {
                next_selections.extend(selections)
            } else {
                next_selections.extend(fragment.selections.iter().cloned())
            };
            selections = TransformedValue::Replace(next_selections);
        }
        match selections {
            TransformedValue::Keep => Transformed::Keep,
            TransformedValue::Replace(selections) => Transformed::Replace(FragmentDefinition {
                selections,
                ..fragment.clone()
            }),
        }
    }

    fn transform_linked_field(&mut self, field: &'s LinkedField) -> Transformed<Selection> {
        let schema = &self.program.schema;
        let field_definition = schema.field(field.definition.item);
        let parent_type = self.parent_type;
        self.parent_type = Some(field_definition.type_.inner());
        let selections = self.transform_selections(&field.selections);
        self.parent_type = parent_type;
        let is_abstract = field_definition.type_.inner().is_abstract_type();
        let selections = if is_abstract && !has_typename_field(schema, &field.selections) {
            let mut next_selections = Vec::with_capacity(field.selections.len() + 1);
            next_selections.push(Selection::ScalarField(Arc::new(ScalarField {
                alias: None,
                definition: WithLocation::new(field.definition.location, schema.typename_field()),
                arguments: Default::default(),
                directives: Default::default(),
            })));
            if let TransformedValue::Replace(selections) = selections {
                next_selections.extend(selections)
            } else {
                next_selections.extend(field.selections.iter().cloned());
            }
            TransformedValue::Replace(next_selections)
        } else {
            selections
        };
        match selections {
            TransformedValue::Keep => Transformed::Keep,
            TransformedValue::Replace(selections) => {
                Transformed::Replace(Selection::LinkedField(Arc::new(LinkedField {
                    alias: field.alias,
                    definition: field.definition,
                    arguments: field.arguments.clone(),
                    directives: field.directives.clone(),
                    selections,
                })))
            }
        }
    }

    fn transform_inline_fragment(
        &mut self,
        fragment: &'s InlineFragment,
    ) -> Transformed<Selection> {
        if let Some(prev) = self.seen.get(fragment) {
            return prev.clone();
        }
        self.seen.insert(fragment, Transformed::Delete);
        let parent_type = self.parent_type;
        if fragment.type_condition.is_some() {
            self.parent_type = fragment.type_condition;
        }
        let mut selections = self.transform_selections(&fragment.selections);
        self.parent_type = parent_type;
        let schema = &self.program.schema;
        let type_ = if let Some(type_) = fragment.type_condition {
            type_
        } else {
            parent_type.expect("Expect the parent type to exist.")
        };
        if !fragment
            .directives
            .iter()
            .any(is_relay_custom_inline_fragment_directive)
            && !schema.is_extension_type(type_)
            && type_.is_abstract_type()
        {
            let mut next_selections = Vec::with_capacity(fragment.selections.len() + 1);
            next_selections.push(generate_abstract_key_field(
                schema,
                type_,
                Location::generated(),
                self.is_for_codegen,
            ));
            if let TransformedValue::Replace(selections) = selections {
                next_selections.extend(selections)
            } else {
                next_selections.extend(fragment.selections.iter().cloned())
            };
            selections = TransformedValue::Replace(next_selections);
        }
        let result = match selections {
            TransformedValue::Keep => Transformed::Keep,
            TransformedValue::Replace(selections) => {
                Transformed::Replace(Selection::InlineFragment(Arc::new(InlineFragment {
                    type_condition: fragment.type_condition,
                    directives: fragment.directives.clone(),
                    selections,
                    spread_location: fragment.spread_location,
                })))
            }
        };
        self.seen.insert(fragment, result.clone());
        result
    }

    fn transform_scalar_field(&mut self, _field: &ScalarField) -> Transformed<Selection> {
        Transformed::Keep
    }

    fn transform_fragment_spread(&mut self, _spread: &FragmentSpread) -> Transformed<Selection> {
        Transformed::Keep
    }
}

fn has_typename_field(schema: &SDLSchema, selections: &[Selection]) -> bool {
    let typename_field = schema.typename_field();
    selections.iter().any(|x| match x {
        Selection::ScalarField(child) => {
            child.alias.is_none() && child.definition.item == typename_field
        }
        _ => false,
    })
}

fn generate_abstract_key_field(
    schema: &SDLSchema,
    type_: Type,
    location: Location,
    is_for_codegen: bool,
) -> Selection {
    let abstract_key = generate_abstract_type_refinement_key(schema, type_);
    Selection::ScalarField(Arc::new(ScalarField {
        alias: Some(WithLocation::new(location, abstract_key)),
        definition: WithLocation::new(location, schema.typename_field()),
        arguments: vec![],
        directives: if is_for_codegen {
            vec![Directive {
                name: WithLocation::new(location, *TYPE_DISCRIMINATOR_DIRECTIVE_NAME),
                arguments: vec![],
                data: None,
                location,
            }]
        } else {
            vec![]
        },
    }))
}
