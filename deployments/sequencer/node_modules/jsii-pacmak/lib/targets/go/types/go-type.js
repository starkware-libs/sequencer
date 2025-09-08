"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.GoType = void 0;
const runtime_1 = require("../runtime");
class GoType {
    constructor(pkg, type) {
        this.pkg = pkg;
        this.type = type;
        this.name = type.name;
        // Prefix with the nesting parent name(s), using an _ delimiter.
        for (let parent = type.nestingParent; parent != null; parent = parent.nestingParent) {
            this.name = `${parent.name}_${this.name}`;
        }
        // Add "jsiiProxy_" prefix to private struct name to avoid keyword conflicts
        // such as "default". See https://github.com/aws/jsii/issues/2637
        this.proxyName = `jsiiProxy_${this.name}`;
        this.fqn = type.fqn;
        this.apiLocation = { api: 'type', fqn: this.fqn };
    }
    get structValidator() {
        return undefined;
    }
    get namespace() {
        return this.pkg.packageName;
    }
    emitDocs(context) {
        context.documenter.emit(this.type.docs, this.apiLocation);
    }
    emitStability(context) {
        context.documenter.emitStability(this.type.docs);
    }
    emitProxyMakerFunction(code, bases) {
        code.open('func() interface{} {');
        if (bases.length > 0) {
            const instanceVar = this.proxyName[0];
            code.line(`${instanceVar} := ${this.proxyName}{}`);
            for (const base of bases) {
                const baseEmbed = this.pkg.resolveEmbeddedType(base);
                code.line(`${runtime_1.JSII_RT_ALIAS}.InitJsiiProxy(&${instanceVar}.${baseEmbed.fieldName})`);
            }
            code.line(`return &${instanceVar}`);
        }
        else {
            code.line(`return &${this.proxyName}{}`);
        }
        // This is always used as a function argument, so we add a trailing comma
        code.close('},');
    }
}
exports.GoType = GoType;
//# sourceMappingURL=go-type.js.map