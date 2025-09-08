"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Calculator = void 0;
/**
 * A sophisticaed multi-language calculator
 */
class Calculator {
    /**
     * Adds the two operands
     * @param ops operands
     */
    add(ops) {
        return ops.lhs + ops.rhs;
    }
    /**
     * Subtracts the two operands
     * @param ops operands
     */
    sub(ops) {
        return ops.lhs - ops.rhs;
    }
    /**
     * Multiplies the two operands
     * @param ops operands
     */
    mul(ops) {
        return ops.lhs * ops.rhs;
    }
}
exports.Calculator = Calculator;
//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoibWFpbi5qcyIsInNvdXJjZVJvb3QiOiIiLCJzb3VyY2VzIjpbIm1haW4udHMiXSwibmFtZXMiOltdLCJtYXBwaW5ncyI6Ijs7O0FBZUE7O0dBRUc7QUFDSCxNQUFhLFVBQVU7SUFDckI7OztPQUdHO0lBQ0ksR0FBRyxDQUFDLEdBQWE7UUFDdEIsT0FBTyxHQUFHLENBQUMsR0FBRyxHQUFHLEdBQUcsQ0FBQyxHQUFHLENBQUM7SUFDM0IsQ0FBQztJQUVEOzs7T0FHRztJQUNJLEdBQUcsQ0FBQyxHQUFhO1FBQ3RCLE9BQU8sR0FBRyxDQUFDLEdBQUcsR0FBRyxHQUFHLENBQUMsR0FBRyxDQUFDO0lBQzNCLENBQUM7SUFFRDs7O09BR0c7SUFDSSxHQUFHLENBQUMsR0FBYTtRQUN0QixPQUFPLEdBQUcsQ0FBQyxHQUFHLEdBQUcsR0FBRyxDQUFDLEdBQUcsQ0FBQTtJQUMxQixDQUFDO0NBQ0Y7QUF4QkQsZ0NBd0JDIiwic291cmNlc0NvbnRlbnQiOlsiLyoqXG4gKiBNYXRoIG9wZXJhbmRzXG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgT3BlcmFuZHMge1xuICAvKipcbiAgICogTGVmdC1oYW5kIHNpZGUgb3BlcmFuZFxuICAgKi9cbiAgcmVhZG9ubHkgbGhzOiBudW1iZXI7XG5cbiAgLyoqXG4gICAqIFJpZ2h0LWhhbmQgc2lkZSBvcGVyYW5kXG4gICAqL1xuICByZWFkb25seSByaHM6IG51bWJlcjtcbn1cblxuLyoqXG4gKiBBIHNvcGhpc3RpY2FlZCBtdWx0aS1sYW5ndWFnZSBjYWxjdWxhdG9yXG4gKi9cbmV4cG9ydCBjbGFzcyBDYWxjdWxhdG9yIHtcbiAgLyoqXG4gICAqIEFkZHMgdGhlIHR3byBvcGVyYW5kc1xuICAgKiBAcGFyYW0gb3BzIG9wZXJhbmRzXG4gICAqL1xuICBwdWJsaWMgYWRkKG9wczogT3BlcmFuZHMpIHtcbiAgICByZXR1cm4gb3BzLmxocyArIG9wcy5yaHM7XG4gIH1cblxuICAvKipcbiAgICogU3VidHJhY3RzIHRoZSB0d28gb3BlcmFuZHNcbiAgICogQHBhcmFtIG9wcyBvcGVyYW5kc1xuICAgKi9cbiAgcHVibGljIHN1YihvcHM6IE9wZXJhbmRzKSB7XG4gICAgcmV0dXJuIG9wcy5saHMgLSBvcHMucmhzO1xuICB9XG4gIFxuICAvKipcbiAgICogTXVsdGlwbGllcyB0aGUgdHdvIG9wZXJhbmRzXG4gICAqIEBwYXJhbSBvcHMgb3BlcmFuZHNcbiAgICovXG4gIHB1YmxpYyBtdWwob3BzOiBPcGVyYW5kcykge1xuICAgIHJldHVybiBvcHMubGhzICogb3BzLnJoc1xuICB9XG59XG4iXX0=