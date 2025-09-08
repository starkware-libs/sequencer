# Contribution Guide

## Breaking Changes

Consumers of this library use it to generate code which is then exposed to customers.

> See https://github.com/cdk8s-team/cdk8s-cli

Therefore, breaking changes in this library will most likely have an affect on the public API of its consumers. This means that it might be impossible for consumers to pull in such changes. 

> Note that this differs from regular libraries, where consumers might have to change the way they interact with the library, but can still preserve their own public API.

In such cases, said consumers are left behind and can no longer take advantage of further enhancement to this library, even though they might be non breaking. For this reason, we treat breaking changes extra carefully. 

As a rule of thumb, if your PR introduces breaking changes:

- Is it crucial to change default behavior? Can we make it a configuration option instead?
- If a default behavior change is needed, try to still provide a configuration option that reverts to the existing behavior.