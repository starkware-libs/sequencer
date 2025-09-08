export function substitute(s: string, variables: { [key: string]: string } = { }) {
  // '': '' is an empty substitution
  for (const [key, value] of Object.entries({ ...variables, '': '' })) {
    s = s.replace(new RegExp(`{{ *${escapeRegExp(key)} *}}`, 'g'), value);
  }
  return s;
}

// https://stackoverflow.com/questions/3446170/escape-string-for-use-in-javascript-regex
function escapeRegExp(s: string) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); // $& means the whole matched string
}