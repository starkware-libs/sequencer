"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.StaticSetProperty = exports.StaticGetProperty = exports.SetProperty = exports.GetProperty = void 0;
const constants_1 = require("./constants");
const function_call_1 = require("./function-call");
const util_1 = require("./util");
class GetProperty extends function_call_1.FunctionCall {
    constructor(parent) {
        super(parent);
        this.parent = parent;
    }
    emit(code) {
        const resultVar = (0, util_1.slugify)('returns', [this.parent.instanceArg]);
        code.line(`var ${resultVar} ${this.returnType}`);
        code.open(`${constants_1.JSII_GET_FUNC}(`);
        code.line(`${this.parent.instanceArg},`);
        code.line(`"${this.parent.property.name}",`);
        code.line(`&${resultVar},`);
        code.close(`)`);
        code.line(`return ${resultVar}`);
    }
}
exports.GetProperty = GetProperty;
class SetProperty {
    constructor(parent) {
        this.parent = parent;
    }
    emit({ code, runtimeTypeChecking }) {
        if (runtimeTypeChecking)
            this.parent.validator?.emitCall(code);
        code.open(`${constants_1.JSII_SET_FUNC}(`);
        code.line(`${this.parent.instanceArg},`);
        code.line(`"${this.parent.property.name}",`);
        code.line(`val,`);
        code.close(`)`);
    }
}
exports.SetProperty = SetProperty;
class StaticGetProperty extends function_call_1.FunctionCall {
    constructor(parent) {
        super(parent);
        this.parent = parent;
    }
    emit(code) {
        (0, util_1.emitInitialization)(code);
        const resultVar = (0, util_1.slugify)('returns', []);
        code.line(`var ${resultVar} ${this.returnType}`);
        code.open(`${constants_1.JSII_SGET_FUNC}(`);
        code.line(`"${this.parent.parent.fqn}",`);
        code.line(`"${this.parent.property.name}",`);
        code.line(`&${resultVar},`);
        code.close(`)`);
        code.line(`return ${resultVar}`);
    }
}
exports.StaticGetProperty = StaticGetProperty;
class StaticSetProperty {
    constructor(parent) {
        this.parent = parent;
    }
    emit({ code, runtimeTypeChecking }) {
        (0, util_1.emitInitialization)(code);
        if (runtimeTypeChecking) {
            this.parent.validator?.emitCall(code);
        }
        code.open(`${constants_1.JSII_SSET_FUNC}(`);
        code.line(`"${this.parent.parent.fqn}",`);
        code.line(`"${this.parent.property.name}",`);
        code.line(`val,`);
        code.close(`)`);
    }
}
exports.StaticSetProperty = StaticSetProperty;
//# sourceMappingURL=property-access.js.map