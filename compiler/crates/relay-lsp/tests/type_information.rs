/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use fixture_tests::Fixture;
use graphql_ir::reexport::StringKey;
use itertools::Itertools;
use relay_lsp::type_information::print_type;
use relay_test_schema::get_test_schema_with_extensions;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize, Debug)]
struct TestSettings {
    type_name: StringKey,
}

pub async fn transform_fixture(_fixture: &Fixture<'_>) -> Result<String, String> {
    let settings: TestSettings = serde_json::from_str(_fixture.content).unwrap();

    let lots_of_fields = (0..1000)
        .map(|i| format!("  field{}: String\n", i))
        .join("");

    let schema = get_test_schema_with_extensions(&format!(
        r"
type MassiveType {{
{lots_of_fields}
}}
"
    ));

    let type_ = *schema
        .get_type_map()
        .find_map(|(&name, type_)| (name == settings.type_name).then_some(type_))
        .unwrap();

    Ok(print_type(&schema, type_))
}
