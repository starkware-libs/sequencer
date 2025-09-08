# jsii-srcmak

> Generates [jsii] source files for multiple languages from TypeScript.

[jsii]: https://github.com/aws/jsii

## Usage

This package can be either used as a library or through a CLI.

The library entry point is the `srcmak` function:

```ts
import { srcmak } from 'jsii-srcmak';
await srcmak(srcdir[, options]);
```

The CLI is `jsii-srcmak`:

```bash
$ jsii-srcmak srcdir [OPTIONS]
```

The `srcdir` argument points to a directory tree that includes TypeScript files
which will be translated through jsii to one of the supported languages.

### Compile only

If called with no additional arguments, `srcmak` will only jsii-compile the source. If compilation fails, it will throw an error. This is a nice way to check if generated typescript code is jsii-compatible:

```ts
const srcdir = generateSomeTypeScriptCode();

// verify it is jsii-compatible (throws otherwise)
await srcmak(srcdir);
```

CLI:

```bash
$ jsii-srcmak /source/directory
```

### Python Output

To produce a Python module from your source, use the `python` option:

```ts
await srcmak('srcdir', {
  python: {
    outdir: '/path/to/project/root',
    moduleName: 'name.of.python.module'
  }
});
```

Or the `--python-*` switches in the CLI:

```bash
$ jsii-srcmak /src/dir --python-outdir=dir --python-module-name=module.name
```

* The `outdir`/`--python-outdir` option points to the root directory of your Python project.
* The `moduleName`/`--python-module-name` option is the python module name. Dots (`.`) delimit submodules.

The output directory will include a python module that corresponds to the
original module. This code depends on the following python modules:

- [jsii](https://pypi.org/project/jsii/)

### Java Output

To produce a Java module from your source, use the `java` option:

```ts
await srcmak('srcdir', {
  java: {
    outdir: '/path/to/project/root',
    package: 'hello.world'
  }
});
```

Or the `--java-*` switches in the CLI:

```bash
$ jsii-srcmak /src/dir --java-outdir=dir --java-package=hello.world
```

* The `outdir`/`--java-outdir` option points to the root directory of your Java project.
* The `package`/`--java-package` option is the java package name.

The output directory will include a java module that corresponds to the
original module. This code depends on the following maven package (should be defined directly or indirectly in the project's `pom.xml` file):

- [jsii](https://mvnrepository.com/artifact/software.amazon.jsii)

The output directory will also include a tarball `generated@0.0.0.jsii.tgz` that must be bundled in your project.

### C# Output

To produce a C# module from your source, use the `csharp` option:

```ts
await srcmak('srcdir', {
  csharp: {
    outdir: '/path/to/project/root',
    namespace: 'HelloWorld'
  }
});
```

Or the `--csharp-*` switches in the CLI:

```bash
$ jsii-srcmak /src/dir --csharp-outdir=dir --csharp-namespace=HelloWorld
```

* The `outdir`/`--csharp-outdir` option points to the root directory of your C# project.
* The `package`/`--csharp-namespace` option is the C# root namespace.

The output directory will include a C# project that corresponds to the
original module. This code depends on the following NuGet package (It is already defined as a dependency in the generated project):

- [jsii](https://www.nuget.org/packages/Amazon.JSII.Runtime/)

The output directory will also include a tarball `generated@0.0.0.jsii.tgz` that must be bundled in your project (It is already included as an embedded resource in the generated project).

### Go Output

To produce a Go module from your source, use the `golang` option:

```ts
await srcmak('srcdir', {
  golang: {
    outdir: '/path/to/project/root',
    moduleName: 'github.com/yourorg/your-root-project',
    packageName: 'helloworld'
  }
});
```

Or the `--golang-*` switches in the CLI:

```bash
$ jsii-srcmak /src/dir --golang-outdir=dir --golang-module="github.com/yourorg/your-root-project" --golang-package="helloworld"
```

* The `outdir`/`--golang-outdir` option points to the root directory of your base Go project (where your `go.mod` is in, if you have one).
* The `moduleName`/`--golang-module` option must match the Go module name of the project that includes the generated source code e.g. `github.com/yourorg/your-root-project`. This is currently required, because the generated code needs to reference a submodule which is generated in a nested directory (see also upstream issue https://github.com/aws/jsii/issues/2847 for more information).
* The `packageName`/`--golang-package` is the package in which the generated Go code will be in. It will be placed in the submodule. So the import path becomes e.g. `github.com/yourorg/your-root-project/yourpackage`.

The output directory will include a directory named with the `packageName`/`--golang-package` containing the generated Go code.
This code depends on the following Go module:

- [jsii-runtime-go](github.com/aws/jsii-runtime-go)

which you need to include in your `go.mod`:
```
require github.com/aws/jsii-runtime-go v1.29.0 # update the version to match the jsii version used in your version of jsii-srcmak
```


#### Nested output directories
It is also possible to set the `outdir`/`--golang-outdir` option to a nested directory inside your Go project. For example, if you want to nest the generated code in a directory called `generated`.
In that case you need to append the subdirectory to the module name (e.g. `github.com/yourorg/your-root-project/generated`):

```bash
$ jsii-srcmak /src/dir --golang-outdir=~/projects/your-root-project/generated --golang-module="github.com/yourorg/your-root-project/generated" --golang-package="helloworld"
```
Your import path will then become e.g. `github.com/yourorg/your-root-project/generated/yourpackage`.

### Entrypoint

The `entrypoint` option can be used to customize the name of the typescript entrypoint (default is `index.ts`).

For example, if the code's entry point is under `/srcdir/foobar/lib/index.ts` then I can specify:

```ts
await srcmak('/srcdir', {
  entrypoint: 'foobar/lib/index.ts'
});
```

Or through the CLI:

```bash
$ jsii-srcmak /srcdir --entrypoint lib/main.ts
```

### Dependencies

The `deps` option can be used to specify a list of node module **directories** (must have a `package.json` file) which will be symlinked into the workspace when compiling your code.

This is required if your code references types from other modules.

Use this idiom to resolve a set of modules directories from the calling process:

```ts
const modules = [
  '@types/node', // commonly needed
  'foobar'       // a node module in *my* closure
];

const getModuleDir = m =>
  path.dirname(require.resolve(`${m}/package.json`));

await srcmak('srcdir', {
  deps: modules.map(getModuleDir)
});
```

Or through the CLI:

```bash
$ jsii-srcmak /src/dir --dep node_modules/@types/node --dep node_modules/constructs
```

## Contributing

To build this project, you must first generate the `package.json`:

```
npx projen
```

Then you can install your dependencies and build:

```
yarn install
yarn build
```

## What's with this name?

It's a silly little pun that stems from another pun: jsii has `jsii-pacmak`
which stands for "package maker". That's the tool that takes in a .jsii manifest
and produces language-idiomatic *packages* from it. This tool produces *sources*
from a .jsii manifest. Hence, "source maker". Yeah, it's lame.

## License

Distributed under the [Apache 2.0](./LICENSE) license.
