/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 * @oncall relay
 */

'use strict';

const RelayModernFragmentSpecResolver = require('../RelayModernFragmentSpecResolver');
const {
  createOperationDescriptor,
} = require('../RelayModernOperationDescriptor');
const {graphql} = require('relay-runtime');
const {createMockEnvironment} = require('relay-test-utils-internal');

describe('RelayModernFragmentSpecResolver', () => {
  let UserFragment;
  let UserQuery;
  let context;
  let environment;
  let zuck;
  let zuckOperation;
  let variables;
  let logger;
  let relayFieldLogger;

  function setUserField(id, fieldName, value) {
    environment.applyUpdate({
      storeUpdater: store => {
        const user = store.get(id);
        user.setValue(value, fieldName);
      },
    });
  }

  beforeEach(() => {
    logger = jest.fn();
    relayFieldLogger = jest.fn();
    environment = createMockEnvironment({log: logger, relayFieldLogger});
    UserFragment = graphql`
      fragment RelayModernFragmentSpecResolverRequiredFieldTestUserFragment on User {
        id
        name @required(action: THROW)
        alternate_name @required(action: LOG)
      }
    `;
    UserQuery = graphql`
      query RelayModernFragmentSpecResolverRequiredFieldTestUserQuery(
        $id: ID!
      ) {
        node(id: $id) {
          ...RelayModernFragmentSpecResolverRequiredFieldTestUserFragment
            @dangerously_unaliased_fixme
        }
      }
    `;
    zuckOperation = createOperationDescriptor(UserQuery, {
      id: '4',
    });
    environment.commitPayload(zuckOperation, {
      node: {
        id: '4',
        __typename: 'User',
        name: 'Zuck',
        alternate_name: 'Zuckster',
      },
    });
    zuck = environment.lookup(zuckOperation.fragment, zuckOperation).data.node;
    variables = {
      id: '4',
    };
    context = {environment, variables};
  });

  it('throws when encountering a missing @required(action: THROW) field', () => {
    setUserField('4', 'name', null);
    const resolver = new RelayModernFragmentSpecResolver(
      context,
      {user: UserFragment},
      {user: zuck},
      jest.fn(),
      true /* rootIsQueryRenderer */,
    );
    expect(() => resolver.resolve()).toThrowError(
      "Relay: Missing @required value at path 'name' in 'RelayModernFragmentSpecResolverRequiredFieldTestUserFragment'.",
    );
  });

  it('logs when encountering a missing @required(action: LOG) field', () => {
    setUserField('4', 'alternate_name', null);
    const resolver = new RelayModernFragmentSpecResolver(
      context,
      {user: UserFragment},
      {user: zuck},
      jest.fn(),
      true /* rootIsQueryRenderer */,
    );
    resolver.resolve();
    expect(relayFieldLogger).toHaveBeenCalledWith({
      fieldPath: 'alternate_name',
      kind: 'missing_required_field.log',
      owner: 'RelayModernFragmentSpecResolverRequiredFieldTestUserFragment',
    });
  });

  it('throws when reading a @required(action: THROW) field that has become null', () => {
    const resolver = new RelayModernFragmentSpecResolver(
      context,
      {user: UserFragment},
      {user: zuck},
      jest.fn(),
      true /* rootIsQueryRenderer */,
    );
    const data = resolver.resolve();
    expect(data.user.name).toBe('Zuck');
    setUserField('4', 'name', null);

    expect(() => resolver.resolve()).toThrowError(
      "Relay: Missing @required value at path 'name' in 'RelayModernFragmentSpecResolverRequiredFieldTestUserFragment'.",
    );

    expect(relayFieldLogger).toHaveBeenCalledWith({
      fieldPath: 'name',
      kind: 'missing_required_field.throw',
      owner: 'RelayModernFragmentSpecResolverRequiredFieldTestUserFragment',
      handled: false,
    });
  });
});
