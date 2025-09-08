"use strict";
var _a, _b;
Object.defineProperty(exports, "__esModule", { value: true });
exports.Dependable = exports.DependencyGroup = void 0;
const JSII_RTTI_SYMBOL_1 = Symbol.for("jsii.rtti");
/**
 * A set of constructs to be used as a dependable
 *
 * This class can be used when a set of constructs which are disjoint in the
 * construct tree needs to be combined to be used as a single dependable.
 */
class DependencyGroup {
    constructor(...deps) {
        this._deps = new Array();
        const self = this;
        Dependable.implement(this, {
            get dependencyRoots() {
                const result = new Array();
                for (const d of self._deps) {
                    result.push(...Dependable.of(d).dependencyRoots);
                }
                return result;
            },
        });
        this.add(...deps);
    }
    /**
     * Add a construct to the dependency roots
     */
    add(...scopes) {
        this._deps.push(...scopes);
    }
}
exports.DependencyGroup = DependencyGroup;
_a = JSII_RTTI_SYMBOL_1;
DependencyGroup[_a] = { fqn: "constructs.DependencyGroup", version: "10.4.2" };
const DEPENDABLE_SYMBOL = Symbol.for('@aws-cdk/core.DependableTrait');
/**
 * Trait for IDependable
 *
 * Traits are interfaces that are privately implemented by objects. Instead of
 * showing up in the public interface of a class, they need to be queried
 * explicitly. This is used to implement certain framework features that are
 * not intended to be used by Construct consumers, and so should be hidden
 * from accidental use.
 *
 * @example
 *
 * // Usage
 * const roots = Dependable.of(construct).dependencyRoots;
 *
 * // Definition
 * Dependable.implement(construct, {
 *       dependencyRoots: [construct],
 * });
 */
class Dependable {
    /**
     * Turn any object into an IDependable.
     */
    static implement(instance, trait) {
        // I would also like to reference classes (to cut down on the list of objects
        // we need to manage), but we can't do that either since jsii doesn't have the
        // concept of a class reference.
        instance[DEPENDABLE_SYMBOL] = trait;
    }
    /**
     * Return the matching Dependable for the given class instance.
     */
    static of(instance) {
        const ret = instance[DEPENDABLE_SYMBOL];
        if (!ret) {
            throw new Error(`${instance} does not implement IDependable. Use "Dependable.implement()" to implement`);
        }
        return ret;
    }
    /**
     * Return the matching Dependable for the given class instance.
     * @deprecated use `of`
     */
    static get(instance) {
        return this.of(instance);
    }
}
exports.Dependable = Dependable;
_b = JSII_RTTI_SYMBOL_1;
Dependable[_b] = { fqn: "constructs.Dependable", version: "10.4.2" };
//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoiZGVwZW5kZW5jeS5qcyIsInNvdXJjZVJvb3QiOiIiLCJzb3VyY2VzIjpbIi4uL3NyYy9kZXBlbmRlbmN5LnRzIl0sIm5hbWVzIjpbXSwibWFwcGluZ3MiOiI7Ozs7O0FBaUJBOzs7OztHQUtHO0FBQ0gsTUFBYSxlQUFlO0lBRzFCLFlBQVksR0FBRyxJQUFtQjtRQUZqQixVQUFLLEdBQUcsSUFBSSxLQUFLLEVBQWUsQ0FBQztRQUdoRCxNQUFNLElBQUksR0FBRyxJQUFJLENBQUM7UUFFbEIsVUFBVSxDQUFDLFNBQVMsQ0FBQyxJQUFJLEVBQUU7WUFDekIsSUFBSSxlQUFlO2dCQUNqQixNQUFNLE1BQU0sR0FBRyxJQUFJLEtBQUssRUFBYyxDQUFDO2dCQUN2QyxLQUFLLE1BQU0sQ0FBQyxJQUFJLElBQUksQ0FBQyxLQUFLLEVBQUUsQ0FBQztvQkFDM0IsTUFBTSxDQUFDLElBQUksQ0FBQyxHQUFHLFVBQVUsQ0FBQyxFQUFFLENBQUMsQ0FBQyxDQUFDLENBQUMsZUFBZSxDQUFDLENBQUM7Z0JBQ25ELENBQUM7Z0JBQ0QsT0FBTyxNQUFNLENBQUM7WUFDaEIsQ0FBQztTQUNGLENBQUMsQ0FBQztRQUVILElBQUksQ0FBQyxHQUFHLENBQUMsR0FBRyxJQUFJLENBQUMsQ0FBQztJQUNwQixDQUFDO0lBRUQ7O09BRUc7SUFDSSxHQUFHLENBQUMsR0FBRyxNQUFxQjtRQUNqQyxJQUFJLENBQUMsS0FBSyxDQUFDLElBQUksQ0FBQyxHQUFHLE1BQU0sQ0FBQyxDQUFDO0lBQzdCLENBQUM7O0FBeEJILDBDQXlCQzs7O0FBRUQsTUFBTSxpQkFBaUIsR0FBRyxNQUFNLENBQUMsR0FBRyxDQUFDLCtCQUErQixDQUFDLENBQUM7QUFFdEU7Ozs7Ozs7Ozs7Ozs7Ozs7OztHQWtCRztBQUNILE1BQXNCLFVBQVU7SUFDOUI7O09BRUc7SUFDSSxNQUFNLENBQUMsU0FBUyxDQUFDLFFBQXFCLEVBQUUsS0FBaUI7UUFDOUQsNkVBQTZFO1FBQzdFLDhFQUE4RTtRQUM5RSxnQ0FBZ0M7UUFDL0IsUUFBZ0IsQ0FBQyxpQkFBaUIsQ0FBQyxHQUFHLEtBQUssQ0FBQztJQUMvQyxDQUFDO0lBRUQ7O09BRUc7SUFDSSxNQUFNLENBQUMsRUFBRSxDQUFDLFFBQXFCO1FBQ3BDLE1BQU0sR0FBRyxHQUFJLFFBQWdCLENBQUMsaUJBQWlCLENBQUMsQ0FBQztRQUNqRCxJQUFJLENBQUMsR0FBRyxFQUFFLENBQUM7WUFDVCxNQUFNLElBQUksS0FBSyxDQUFDLEdBQUcsUUFBUSw0RUFBNEUsQ0FBQyxDQUFDO1FBQzNHLENBQUM7UUFDRCxPQUFPLEdBQUcsQ0FBQztJQUNiLENBQUM7SUFFRDs7O09BR0c7SUFDSSxNQUFNLENBQUMsR0FBRyxDQUFDLFFBQXFCO1FBQ3JDLE9BQU8sSUFBSSxDQUFDLEVBQUUsQ0FBQyxRQUFRLENBQUMsQ0FBQztJQUMzQixDQUFDOztBQTVCSCxnQ0FxQ0MiLCJzb3VyY2VzQ29udGVudCI6WyJpbXBvcnQgeyBJQ29uc3RydWN0IH0gZnJvbSAnLi9jb25zdHJ1Y3QnO1xuXG4vKipcbiAqIFRyYWl0IG1hcmtlciBmb3IgY2xhc3NlcyB0aGF0IGNhbiBiZSBkZXBlbmRlZCB1cG9uXG4gKlxuICogVGhlIHByZXNlbmNlIG9mIHRoaXMgaW50ZXJmYWNlIGluZGljYXRlcyB0aGF0IGFuIG9iamVjdCBoYXNcbiAqIGFuIGBJRGVwZW5kYWJsZWAgaW1wbGVtZW50YXRpb24uXG4gKlxuICogVGhpcyBpbnRlcmZhY2UgY2FuIGJlIHVzZWQgdG8gdGFrZSBhbiAob3JkZXJpbmcpIGRlcGVuZGVuY3kgb24gYSBzZXQgb2ZcbiAqIGNvbnN0cnVjdHMuIEFuIG9yZGVyaW5nIGRlcGVuZGVuY3kgaW1wbGllcyB0aGF0IHRoZSByZXNvdXJjZXMgcmVwcmVzZW50ZWQgYnlcbiAqIHRob3NlIGNvbnN0cnVjdHMgYXJlIGRlcGxveWVkIGJlZm9yZSB0aGUgcmVzb3VyY2VzIGRlcGVuZGluZyBPTiB0aGVtIGFyZVxuICogZGVwbG95ZWQuXG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgSURlcGVuZGFibGUge1xuICAvLyBFbXB0eSwgdGhpcyBpbnRlcmZhY2UgaXMgYSB0cmFpdCBtYXJrZXJcbn1cblxuLyoqXG4gKiBBIHNldCBvZiBjb25zdHJ1Y3RzIHRvIGJlIHVzZWQgYXMgYSBkZXBlbmRhYmxlXG4gKlxuICogVGhpcyBjbGFzcyBjYW4gYmUgdXNlZCB3aGVuIGEgc2V0IG9mIGNvbnN0cnVjdHMgd2hpY2ggYXJlIGRpc2pvaW50IGluIHRoZVxuICogY29uc3RydWN0IHRyZWUgbmVlZHMgdG8gYmUgY29tYmluZWQgdG8gYmUgdXNlZCBhcyBhIHNpbmdsZSBkZXBlbmRhYmxlLlxuICovXG5leHBvcnQgY2xhc3MgRGVwZW5kZW5jeUdyb3VwIGltcGxlbWVudHMgSURlcGVuZGFibGUge1xuICBwcml2YXRlIHJlYWRvbmx5IF9kZXBzID0gbmV3IEFycmF5PElEZXBlbmRhYmxlPigpO1xuXG4gIGNvbnN0cnVjdG9yKC4uLmRlcHM6IElEZXBlbmRhYmxlW10pIHtcbiAgICBjb25zdCBzZWxmID0gdGhpcztcblxuICAgIERlcGVuZGFibGUuaW1wbGVtZW50KHRoaXMsIHtcbiAgICAgIGdldCBkZXBlbmRlbmN5Um9vdHMoKSB7XG4gICAgICAgIGNvbnN0IHJlc3VsdCA9IG5ldyBBcnJheTxJQ29uc3RydWN0PigpO1xuICAgICAgICBmb3IgKGNvbnN0IGQgb2Ygc2VsZi5fZGVwcykge1xuICAgICAgICAgIHJlc3VsdC5wdXNoKC4uLkRlcGVuZGFibGUub2YoZCkuZGVwZW5kZW5jeVJvb3RzKTtcbiAgICAgICAgfVxuICAgICAgICByZXR1cm4gcmVzdWx0O1xuICAgICAgfSxcbiAgICB9KTtcblxuICAgIHRoaXMuYWRkKC4uLmRlcHMpO1xuICB9XG5cbiAgLyoqXG4gICAqIEFkZCBhIGNvbnN0cnVjdCB0byB0aGUgZGVwZW5kZW5jeSByb290c1xuICAgKi9cbiAgcHVibGljIGFkZCguLi5zY29wZXM6IElEZXBlbmRhYmxlW10pIHtcbiAgICB0aGlzLl9kZXBzLnB1c2goLi4uc2NvcGVzKTtcbiAgfVxufVxuXG5jb25zdCBERVBFTkRBQkxFX1NZTUJPTCA9IFN5bWJvbC5mb3IoJ0Bhd3MtY2RrL2NvcmUuRGVwZW5kYWJsZVRyYWl0Jyk7XG5cbi8qKlxuICogVHJhaXQgZm9yIElEZXBlbmRhYmxlXG4gKlxuICogVHJhaXRzIGFyZSBpbnRlcmZhY2VzIHRoYXQgYXJlIHByaXZhdGVseSBpbXBsZW1lbnRlZCBieSBvYmplY3RzLiBJbnN0ZWFkIG9mXG4gKiBzaG93aW5nIHVwIGluIHRoZSBwdWJsaWMgaW50ZXJmYWNlIG9mIGEgY2xhc3MsIHRoZXkgbmVlZCB0byBiZSBxdWVyaWVkXG4gKiBleHBsaWNpdGx5LiBUaGlzIGlzIHVzZWQgdG8gaW1wbGVtZW50IGNlcnRhaW4gZnJhbWV3b3JrIGZlYXR1cmVzIHRoYXQgYXJlXG4gKiBub3QgaW50ZW5kZWQgdG8gYmUgdXNlZCBieSBDb25zdHJ1Y3QgY29uc3VtZXJzLCBhbmQgc28gc2hvdWxkIGJlIGhpZGRlblxuICogZnJvbSBhY2NpZGVudGFsIHVzZS5cbiAqXG4gKiBAZXhhbXBsZVxuICpcbiAqIC8vIFVzYWdlXG4gKiBjb25zdCByb290cyA9IERlcGVuZGFibGUub2YoY29uc3RydWN0KS5kZXBlbmRlbmN5Um9vdHM7XG4gKlxuICogLy8gRGVmaW5pdGlvblxuICogRGVwZW5kYWJsZS5pbXBsZW1lbnQoY29uc3RydWN0LCB7XG4gKiAgICAgICBkZXBlbmRlbmN5Um9vdHM6IFtjb25zdHJ1Y3RdLFxuICogfSk7XG4gKi9cbmV4cG9ydCBhYnN0cmFjdCBjbGFzcyBEZXBlbmRhYmxlIHtcbiAgLyoqXG4gICAqIFR1cm4gYW55IG9iamVjdCBpbnRvIGFuIElEZXBlbmRhYmxlLlxuICAgKi9cbiAgcHVibGljIHN0YXRpYyBpbXBsZW1lbnQoaW5zdGFuY2U6IElEZXBlbmRhYmxlLCB0cmFpdDogRGVwZW5kYWJsZSkge1xuICAgIC8vIEkgd291bGQgYWxzbyBsaWtlIHRvIHJlZmVyZW5jZSBjbGFzc2VzICh0byBjdXQgZG93biBvbiB0aGUgbGlzdCBvZiBvYmplY3RzXG4gICAgLy8gd2UgbmVlZCB0byBtYW5hZ2UpLCBidXQgd2UgY2FuJ3QgZG8gdGhhdCBlaXRoZXIgc2luY2UganNpaSBkb2Vzbid0IGhhdmUgdGhlXG4gICAgLy8gY29uY2VwdCBvZiBhIGNsYXNzIHJlZmVyZW5jZS5cbiAgICAoaW5zdGFuY2UgYXMgYW55KVtERVBFTkRBQkxFX1NZTUJPTF0gPSB0cmFpdDtcbiAgfVxuXG4gIC8qKlxuICAgKiBSZXR1cm4gdGhlIG1hdGNoaW5nIERlcGVuZGFibGUgZm9yIHRoZSBnaXZlbiBjbGFzcyBpbnN0YW5jZS5cbiAgICovXG4gIHB1YmxpYyBzdGF0aWMgb2YoaW5zdGFuY2U6IElEZXBlbmRhYmxlKTogRGVwZW5kYWJsZSB7XG4gICAgY29uc3QgcmV0ID0gKGluc3RhbmNlIGFzIGFueSlbREVQRU5EQUJMRV9TWU1CT0xdO1xuICAgIGlmICghcmV0KSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoYCR7aW5zdGFuY2V9IGRvZXMgbm90IGltcGxlbWVudCBJRGVwZW5kYWJsZS4gVXNlIFwiRGVwZW5kYWJsZS5pbXBsZW1lbnQoKVwiIHRvIGltcGxlbWVudGApO1xuICAgIH1cbiAgICByZXR1cm4gcmV0O1xuICB9XG5cbiAgLyoqXG4gICAqIFJldHVybiB0aGUgbWF0Y2hpbmcgRGVwZW5kYWJsZSBmb3IgdGhlIGdpdmVuIGNsYXNzIGluc3RhbmNlLlxuICAgKiBAZGVwcmVjYXRlZCB1c2UgYG9mYFxuICAgKi9cbiAgcHVibGljIHN0YXRpYyBnZXQoaW5zdGFuY2U6IElEZXBlbmRhYmxlKTogRGVwZW5kYWJsZSB7XG4gICAgcmV0dXJuIHRoaXMub2YoaW5zdGFuY2UpO1xuICB9XG5cbiAgLyoqXG4gICAqIFRoZSBzZXQgb2YgY29uc3RydWN0cyB0aGF0IGZvcm0gdGhlIHJvb3Qgb2YgdGhpcyBkZXBlbmRhYmxlXG4gICAqXG4gICAqIEFsbCByZXNvdXJjZXMgdW5kZXIgYWxsIHJldHVybmVkIGNvbnN0cnVjdHMgYXJlIGluY2x1ZGVkIGluIHRoZSBvcmRlcmluZ1xuICAgKiBkZXBlbmRlbmN5LlxuICAgKi9cbiAgcHVibGljIGFic3RyYWN0IHJlYWRvbmx5IGRlcGVuZGVuY3lSb290czogSUNvbnN0cnVjdFtdO1xufVxuIl19