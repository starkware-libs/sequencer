"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.substitute = void 0;
function substitute(s, variables = {}) {
    // '': '' is an empty substitution
    for (const [key, value] of Object.entries({ ...variables, '': '' })) {
        s = s.replace(new RegExp(`{{ *${escapeRegExp(key)} *}}`, 'g'), value);
    }
    return s;
}
exports.substitute = substitute;
// https://stackoverflow.com/questions/3446170/escape-string-for-use-in-javascript-regex
function escapeRegExp(s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); // $& means the whole matched string
}
//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoic3Vic3RpdHV0ZS5qcyIsInNvdXJjZVJvb3QiOiIiLCJzb3VyY2VzIjpbInN1YnN0aXR1dGUudHMiXSwibmFtZXMiOltdLCJtYXBwaW5ncyI6Ijs7O0FBQUEsU0FBZ0IsVUFBVSxDQUFDLENBQVMsRUFBRSxZQUF1QyxFQUFHO0lBQzlFLGtDQUFrQztJQUNsQyxLQUFLLE1BQU0sQ0FBQyxHQUFHLEVBQUUsS0FBSyxDQUFDLElBQUksTUFBTSxDQUFDLE9BQU8sQ0FBQyxFQUFFLEdBQUcsU0FBUyxFQUFFLEVBQUUsRUFBRSxFQUFFLEVBQUUsQ0FBQyxFQUFFO1FBQ25FLENBQUMsR0FBRyxDQUFDLENBQUMsT0FBTyxDQUFDLElBQUksTUFBTSxDQUFDLE9BQU8sWUFBWSxDQUFDLEdBQUcsQ0FBQyxNQUFNLEVBQUUsR0FBRyxDQUFDLEVBQUUsS0FBSyxDQUFDLENBQUM7S0FDdkU7SUFDRCxPQUFPLENBQUMsQ0FBQztBQUNYLENBQUM7QUFORCxnQ0FNQztBQUVELHdGQUF3RjtBQUN4RixTQUFTLFlBQVksQ0FBQyxDQUFTO0lBQzdCLE9BQU8sQ0FBQyxDQUFDLE9BQU8sQ0FBQyxxQkFBcUIsRUFBRSxNQUFNLENBQUMsQ0FBQyxDQUFDLG9DQUFvQztBQUN2RixDQUFDIiwic291cmNlc0NvbnRlbnQiOlsiZXhwb3J0IGZ1bmN0aW9uIHN1YnN0aXR1dGUoczogc3RyaW5nLCB2YXJpYWJsZXM6IHsgW2tleTogc3RyaW5nXTogc3RyaW5nIH0gPSB7IH0pIHtcbiAgLy8gJyc6ICcnIGlzIGFuIGVtcHR5IHN1YnN0aXR1dGlvblxuICBmb3IgKGNvbnN0IFtrZXksIHZhbHVlXSBvZiBPYmplY3QuZW50cmllcyh7IC4uLnZhcmlhYmxlcywgJyc6ICcnIH0pKSB7XG4gICAgcyA9IHMucmVwbGFjZShuZXcgUmVnRXhwKGB7eyAqJHtlc2NhcGVSZWdFeHAoa2V5KX0gKn19YCwgJ2cnKSwgdmFsdWUpO1xuICB9XG4gIHJldHVybiBzO1xufVxuXG4vLyBodHRwczovL3N0YWNrb3ZlcmZsb3cuY29tL3F1ZXN0aW9ucy8zNDQ2MTcwL2VzY2FwZS1zdHJpbmctZm9yLXVzZS1pbi1qYXZhc2NyaXB0LXJlZ2V4XG5mdW5jdGlvbiBlc2NhcGVSZWdFeHAoczogc3RyaW5nKSB7XG4gIHJldHVybiBzLnJlcGxhY2UoL1suKis/XiR7fSgpfFtcXF1cXFxcXS9nLCAnXFxcXCQmJyk7IC8vICQmIG1lYW5zIHRoZSB3aG9sZSBtYXRjaGVkIHN0cmluZ1xufSJdfQ==