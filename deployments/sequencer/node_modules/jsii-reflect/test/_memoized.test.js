"use strict";
var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
Object.defineProperty(exports, "__esModule", { value: true });
const lib_1 = require("../lib");
const _memoized_1 = require("../lib/_memoized");
const accessorSpy = jest.fn(() => 'foobar');
class TestClass {
    constructor(system) {
        this.system = system;
    }
    get uncached() {
        return accessorSpy();
    }
    get cached() {
        return accessorSpy();
    }
    get cachedWhenLocked() {
        return accessorSpy();
    }
}
__decorate([
    _memoized_1.memoized
], TestClass.prototype, "cached", null);
__decorate([
    _memoized_1.memoizedWhenLocked
], TestClass.prototype, "cachedWhenLocked", null);
// eslint-disable-next-line @typescript-eslint/no-empty-function
function noop(_val) { }
describe('memoized', () => {
    beforeEach(() => {
        accessorSpy.mockClear();
    });
    const subject = new TestClass(new lib_1.TypeSystem());
    test('cached property is memoized', () => {
        // Access the property twice
        noop(subject.cached);
        noop(subject.cached);
        expect(accessorSpy).toHaveBeenCalledTimes(1);
        expect(subject.cached).toBe('foobar');
    });
    test('uncached property is not memoized', () => {
        // Access the property twice
        noop(subject.uncached);
        noop(subject.uncached);
        expect(accessorSpy).toHaveBeenCalledTimes(2);
        expect(subject.uncached).toBe('foobar');
    });
});
describe('memoizedWhenLocked', () => {
    let subject;
    beforeEach(() => {
        accessorSpy.mockClear();
        subject = new TestClass(new lib_1.TypeSystem());
    });
    test('property is memoized when the typesystem is locked', () => {
        // Lock the typesystem to enable memoizing
        subject.system.lock();
        // Access the property twice
        noop(subject.cachedWhenLocked);
        noop(subject.cachedWhenLocked);
        expect(accessorSpy).toHaveBeenCalledTimes(1);
        expect(subject.cachedWhenLocked).toBe('foobar');
    });
    test('property is not memoized when the typesystem is not locked', () => {
        // Access the property twice
        noop(subject.cachedWhenLocked);
        noop(subject.cachedWhenLocked);
        expect(accessorSpy).toHaveBeenCalledTimes(2);
        expect(subject.cachedWhenLocked).toBe('foobar');
    });
});
//# sourceMappingURL=_memoized.test.js.map