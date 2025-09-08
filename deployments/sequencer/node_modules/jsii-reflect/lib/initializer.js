"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Initializer = void 0;
const callable_1 = require("./callable");
const type_member_1 = require("./type-member");
class Initializer extends callable_1.Callable {
    constructor() {
        super(...arguments);
        this.kind = type_member_1.MemberKind.Initializer;
        this.name = '<initializer>';
        this.abstract = false;
    }
    static isInitializer(x) {
        return x instanceof Initializer;
    }
}
exports.Initializer = Initializer;
//# sourceMappingURL=initializer.js.map