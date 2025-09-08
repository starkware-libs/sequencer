"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const reflect = require("../lib");
const util_1 = require("./util");
test('get full github source location for a class or method', () => {
    // WHEN
    const assembly = (0, util_1.assemblyFromSource)(`
    export class Foo {
      public bar() {
        // Nothing to do
      }
    }
  `.trim(), (obj) => {
        if (typeof obj.repository === 'object') {
            obj.repository.directory = 'some/sub/dir';
        }
    });
    // THEN
    const klass = assembly.findType('testpkg.Foo');
    expect(klass.isClassType).toBeTruthy();
    expect(reflect.repositoryUrl(klass, 'main')).toBe('https://github.com/aws/jsii/blob/main/some/sub/dir/index.ts#L1');
    expect(reflect.repositoryUrl(klass)).toBe('https://github.com/aws/jsii/blob/master/some/sub/dir/index.ts#L1');
});
//# sourceMappingURL=independent.test.js.map