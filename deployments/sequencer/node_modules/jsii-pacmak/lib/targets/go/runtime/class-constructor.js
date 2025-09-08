"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ClassConstructor = void 0;
const constants_1 = require("./constants");
const emit_arguments_1 = require("./emit-arguments");
const util_1 = require("./util");
class ClassConstructor {
    constructor(parent) {
        this.parent = parent;
    }
    emit({ code, runtimeTypeChecking }) {
        (0, util_1.emitInitialization)(code);
        code.line();
        if (runtimeTypeChecking) {
            this.parent.validator?.emitCall(code);
        }
        const resultVar = (0, util_1.slugify)(this.parent.parent.proxyName[0], this.parent.parameters.map((p) => p.name));
        const args = (0, emit_arguments_1.emitArguments)(code, this.parent.parameters, resultVar);
        code.line(`${resultVar} := ${this.parent.parent.proxyName}{}`);
        code.line();
        code.open(`${constants_1.JSII_CREATE_FUNC}(`);
        code.line(`"${this.parent.parent.fqn}",`);
        code.line(args ? `${args},` : 'nil, // no parameters');
        code.line(`&${resultVar},`);
        code.close(`)`);
        code.line();
        code.line(`return &${resultVar}`);
    }
    emitOverride(code, instanceVar) {
        (0, util_1.emitInitialization)(code);
        code.line();
        const args = (0, emit_arguments_1.emitArguments)(code, this.parent.parameters, instanceVar);
        code.open(`${constants_1.JSII_CREATE_FUNC}(`);
        code.line(`"${this.parent.parent.fqn}",`);
        code.line(args ? `${args},` : 'nil, // no parameters');
        code.line(`${instanceVar},`);
        code.close(')');
    }
}
exports.ClassConstructor = ClassConstructor;
//# sourceMappingURL=class-constructor.js.map