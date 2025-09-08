const { execSync } = require('child_process');
const { readFileSync } = require('fs');
const { platform } = require('os');

const cli = require.resolve('../../bin/cdk8s');

exports.pre = () => {
  try {
    execSync(`${platform() === 'win32' ? 'where' : 'which'} go`);
  } catch {
    console.error(`Unable to find "go". Install from https://golang.org/`);
    process.exit(1);
  }
};

exports.post = options => {
  execSync(`node "${cli}" import k8s -l go`);

  // used to generate go.sum file which tracks hashes of all dependencies
  execSync('go mod tidy');

  execSync('go run .');

  console.log(readFileSync('./help', 'utf-8'));
};

