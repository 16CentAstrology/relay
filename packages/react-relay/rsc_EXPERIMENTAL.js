/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 * @oncall relay
 */

'use strict';

import type {
  Fragment,
  FragmentType,
  IEnvironment,
  Query,
  Variables,
} from 'relay-runtime';

const serverFetchQueryImpl = require('./relay-hooks/rsc/serverFetchQuery');
const serverReadFragmentImpl = require('./relay-hooks/rsc/serverReadFragment');
const invariant = require('invariant');
const React = require('react');

type HasSpread<TFragmentType> = {+$fragmentSpreads: TFragmentType, ...};

export type ServerEnvironment = {
  +getEnvironment: () => IEnvironment,
  +serverFetchQuery: <TVariables extends Variables, TData>(
    query: Query<TVariables, TData>,
    variables: TVariables,
  ) => Promise<TData>,
  +serverReadFragment: <TFragmentType extends FragmentType, TData>(
    fragment: Fragment<TFragmentType, TData>,
    fragmentRef:
      | HasSpread<TFragmentType>
      | ReadonlyArray<HasSpread<TFragmentType>>,
  ) => Promise<TData>,
};

function createServerEnvironment(
  create: () => IEnvironment,
): ServerEnvironment {
  // $FlowFixMe[missing-export] React.cache is available in React 19+
  const cache = React.cache;
  invariant(typeof cache === 'function', 'Relay RSC APIs require React 19+');
  const getEnvironment = cache(create);

  async function serverFetchQuery<TVariables extends Variables, TData>(
    query: Query<TVariables, TData>,
    variables: TVariables,
  ): Promise<TData> {
    return serverFetchQueryImpl(getEnvironment(), query, variables);
  }

  async function serverReadFragment<TFragmentType extends FragmentType, TData>(
    fragment: Fragment<TFragmentType, TData>,
    fragmentRef:
      | HasSpread<TFragmentType>
      | ReadonlyArray<HasSpread<TFragmentType>>,
  ): Promise<TData> {
    return serverReadFragmentImpl(getEnvironment(), fragment, fragmentRef);
  }

  return {
    getEnvironment,
    serverFetchQuery,
    serverReadFragment,
  };
}

module.exports = {createServerEnvironment};
