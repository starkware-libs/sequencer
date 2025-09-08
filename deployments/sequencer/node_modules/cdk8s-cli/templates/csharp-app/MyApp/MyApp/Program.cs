using Constructs;
using Org.Cdk8s;

var app = new App();
_ = new Main(app, "{{ $base }}");

app.Synth();

public class Main(Construct scope, string id, IChartProps? props = null) : Chart(scope, id, props);