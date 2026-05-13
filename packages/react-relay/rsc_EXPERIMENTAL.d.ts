/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import {GraphQLTaggedNode, IEnvironment, OperationType} from 'relay-runtime';

export interface ServerEnvironment {
    readonly getEnvironment: () => IEnvironment;
    readonly serverFetchQuery: <T extends OperationType>(
        query: GraphQLTaggedNode,
        variables: T['variables'],
    ) => Promise<T['response']>;
}

export function createServerEnvironment(
    create: () => IEnvironment,
): ServerEnvironment;
