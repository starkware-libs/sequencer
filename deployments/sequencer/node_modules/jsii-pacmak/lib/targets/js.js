"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const spec = require("@jsii/spec");
const generator_1 = require("../generator");
const target_1 = require("../target");
const version_utils_1 = require("./version-utils");
const _1 = require(".");
class JavaScript extends target_1.Target {
    constructor() {
        super(...arguments);
        this.generator = new PackOnly();
    }
    static toPackageInfos(assm) {
        const releaseVersion = (0, version_utils_1.toReleaseVersion)(assm.version, _1.TargetName.JAVASCRIPT);
        const packageInfo = {
            repository: 'NPM',
            url: `https://www.npmjs.com/package/${assm.name}/v/${releaseVersion}`,
            usage: {
                'package.json': {
                    language: 'js',
                    code: JSON.stringify({ [assm.name]: `^${releaseVersion}` }, null, 2),
                },
                npm: {
                    language: 'console',
                    code: `$ npm i ${assm.name}@${releaseVersion}`,
                },
                yarn: {
                    language: 'console',
                    code: `$ yarn add ${assm.name}@${releaseVersion}`,
                },
            },
        };
        return { typescript: packageInfo, javascript: packageInfo };
    }
    static toNativeReference(type) {
        const [, ...name] = type.fqn.split('.');
        const resolvedName = name.join('.');
        const result = {
            typescript: `import { ${resolvedName} } from '${type.assembly}';`,
        };
        if (!spec.isInterfaceType(type)) {
            result.javascript = `const { ${resolvedName} } = require('${type.assembly}');`;
        }
        else {
            result.javascript = `// ${resolvedName} is an interface`;
        }
        return result;
    }
    async build(sourceDir, outDir) {
        return this.copyFiles(sourceDir, outDir);
    }
}
exports.default = JavaScript;
// ##################
// # CODE GENERATOR #
// ##################
class PackOnly extends generator_1.Generator {
    constructor() {
        // NB: This does not generate code, so runtime type checking is irrelevant
        super({ runtimeTypeChecking: false });
    }
    async save(outdir, tarball, _) {
        // Intentionally ignore the Legalese field here... it's not useful here.
        return super.save(outdir, tarball, {});
    }
    getAssemblyOutputDir(_mod) {
        return '.';
    }
    onBeginInterface(_ifc) {
        return;
    }
    onEndInterface(_ifc) {
        return;
    }
    onInterfaceMethod(_ifc, _method) {
        return;
    }
    onInterfaceMethodOverload(_ifc, _overload, _originalMethod) {
        return;
    }
    onInterfaceProperty(_ifc, _prop) {
        return;
    }
    onProperty(_cls, _prop) {
        return;
    }
    onStaticProperty(_cls, _prop) {
        return;
    }
    onUnionProperty(_cls, _prop, _union) {
        return;
    }
    onMethod(_cls, _method) {
        return;
    }
    onMethodOverload(_cls, _overload, _originalMethod) {
        return;
    }
    onStaticMethod(_cls, _method) {
        return;
    }
    onStaticMethodOverload(_cls, _overload, _originalMethod) {
        return;
    }
}
//# sourceMappingURL=js.js.map