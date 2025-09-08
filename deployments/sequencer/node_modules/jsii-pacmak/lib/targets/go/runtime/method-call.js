"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.MethodCall = void 0;
const jsii_reflect_1 = require("jsii-reflect");
const constants_1 = require("./constants");
const emit_arguments_1 = require("./emit-arguments");
const function_call_1 = require("./function-call");
const util_1 = require("./util");
class MethodCall extends function_call_1.FunctionCall {
    constructor(parent) {
        super(parent);
        this.parent = parent;
        this._returnVarName = '';
    }
    emit(context) {
        if (this.inStatic) {
            this.emitStatic(context);
        }
        else {
            this.emitDynamic(context);
        }
    }
    emitDynamic({ code, runtimeTypeChecking }) {
        if (runtimeTypeChecking) {
            this.parent.validator?.emitCall(code);
        }
        const args = (0, emit_arguments_1.emitArguments)(code, this.parent.parameters, this.returnVarName);
        if (this.returnsVal) {
            code.line(`var ${this.returnVarName} ${this.returnType}`);
            code.line();
            code.open(`${constants_1.JSII_INVOKE_FUNC}(`);
        }
        else {
            code.open(`${constants_1.JSII_INVOKE_VOID_FUNC}(`);
        }
        code.line(`${this.parent.instanceArg},`);
        code.line(`"${this.parent.method.name}",`);
        code.line(args ? `${args},` : 'nil, // no parameters');
        if (this.returnsVal) {
            code.line(`&${this.returnVarName},`);
        }
        code.close(`)`);
        if (this.returnsVal) {
            code.line();
            code.line(`return ${this.returnVarName}`);
        }
    }
    emitStatic({ code, runtimeTypeChecking }) {
        (0, util_1.emitInitialization)(code);
        code.line();
        if (runtimeTypeChecking) {
            this.parent.validator?.emitCall(code);
        }
        const args = (0, emit_arguments_1.emitArguments)(code, this.parent.parameters, this.returnVarName);
        if (this.returnsVal) {
            code.line(`var ${this.returnVarName} ${this.returnType}`);
            code.line();
            code.open(`${constants_1.JSII_SINVOKE_FUNC}(`);
        }
        else {
            code.open(`${constants_1.JSII_SINVOKE_VOID_FUNC}(`);
        }
        code.line(`"${this.parent.parent.fqn}",`);
        code.line(`"${this.parent.method.name}",`);
        code.line(args ? `${args},` : 'nil, // no parameters');
        if (this.returnsVal) {
            code.line(`&${this.returnVarName},`);
        }
        code.close(`)`);
        if (this.returnsVal) {
            code.line();
            code.line(`return ${this.returnVarName}`);
        }
    }
    get returnVarName() {
        if (this._returnVarName === '') {
            this._returnVarName = (0, util_1.slugify)('returns', this.parent.parameters.map((p) => p.name));
        }
        return this._returnVarName;
    }
    get inStatic() {
        return jsii_reflect_1.Method.isMethod(this.parent.method) && this.parent.method.static;
    }
}
exports.MethodCall = MethodCall;
//# sourceMappingURL=method-call.js.map