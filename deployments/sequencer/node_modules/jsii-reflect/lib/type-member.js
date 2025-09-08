"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.MemberKind = void 0;
exports.isInitializer = isInitializer;
exports.isMethod = isMethod;
exports.isProperty = isProperty;
var MemberKind;
(function (MemberKind) {
    MemberKind["Initializer"] = "initializer";
    MemberKind["Method"] = "method";
    MemberKind["Property"] = "property";
})(MemberKind || (exports.MemberKind = MemberKind = {}));
function isInitializer(x) {
    return x.kind === MemberKind.Initializer;
}
function isMethod(x) {
    return x.kind === MemberKind.Method;
}
function isProperty(x) {
    return x.kind === MemberKind.Property;
}
//# sourceMappingURL=type-member.js.map