const { execSync } = require('child_process');
const { readFileSync } = require('fs');

exports.post = ctx => {

  const env = { ...process.env };

  execSync('npm install', { stdio: 'inherit', env });

  // import k8s objects
  execSync('npm run import', { stdio: 'inherit', env });
  execSync('npm run compile', { stdio: 'inherit', env });
  execSync('npm run test -- -u', { stdio: 'inherit', env });
  execSync('npm run synth', { stdio: 'inherit', env });

  console.log(readFileSync('./help', 'utf-8'));
};
