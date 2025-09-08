"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.FunctionCall = void 0;
class FunctionCall {
    constructor(parent) {
        this.parent = parent;
    }
    get returnsVal() {
        return !this.parent.reference?.void;
    }
    get returnType() {
        return this.parent.returnType || 'interface{}';
    }
}
exports.FunctionCall = FunctionCall;
//# sourceMappingURL=function-call.js.map