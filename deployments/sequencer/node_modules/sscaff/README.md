# sscaff

Stupid scaffolding: copies an entire directory with variable substitution and
pre/post node.js hooks.

## Installation

```shell
yarn add sscaff
```

or:

```shell
npm install sscaff
```

## Usage

Create a template directory with files and subdirectories. For example:

```
my-first-template
  {{name}}.txt
    Hello, my name is {{name}}!
```

Now, use `sscaff` to create a copy and substitute:

```ts
import { sscaff } from 'sscaff';

await sscaff('my-first-template', 'outdir', {
  name: 'oliver'
});
```

This will create the following:

```
outdir
  oliver.txt
    Hello, my name is oliver!
```

## Built-in Substitutions

* `{{ $base }}` will be substituted by the base name of the output directory
  (e.g. `outdir` in the example above).
* `{{ }}` will be substituted by the empty string. You can use this substitution
  to "salt" a file name so it won't be recognized by the parent project. For
  example, if you want your template to include a gitignore file, you should
  call it `{{}}.gitignore` so it won't be recognized as a "real" gitignore in
  the parent project.

## Hooks

If the template directory has a file named `.hooks.sscaff.js`, and exports `pre`
and/or `post` functions, those will be called before and after the creation of
the output, respectively.

These functions are both executed with the output directory as the working
directory and accept the `variables` dictionary. Both functions can either be
synchronous or asynchronous.

The `pre` function may also modify the `variables` dictionary (i.e. add
variables, modify them, etc).

For example, let's add the following `.hooks.sscaff.js` file to
`my-first-template` above.

```js
const fs = require('fs').promises;

exports.pre = variables => {
  variables.orig = variables.name;
  variables.name = variables.name[0].toUpperCase() + variables.name.slice(1);
};

exports.post = async (variables) => {
  await fs.writeFile(variables.orig + '.bak', 'hello hello');
};
```

The resulting output will look like this now:

```
outdir
  Oliver.txt
    Hello, my name is Oliver!
  oliver.bak
    hello hello
```

## Contributions

All contributions are welcome, just raise an issue or submit a PR. Add a test, update readme. Do the right thing.

## License

[Apache 2.0](./LICENSE)

