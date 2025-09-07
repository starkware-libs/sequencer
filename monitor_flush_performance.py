#!/usr/bin/env python3

"""
Kubernetes Flush Performance Monitor.
"""

import subprocess
import time
import re
import sys
from datetime import datetime
from typing import Dict, List, Optional

class FlushPerformanceMonitor:
    def __init__(self, namespace: str = "sequencer"):
        self.namespace = namespace
        self.pod_name = None
        self.metrics_data = []
        
    def find_sequencer_pod(self) -> Optional[str]:
        """Find the running sequencer pod"""
        try:
            result = subprocess.run([
                "kubectl", "get", "pods", "-n", self.namespace, 
                "-o", "jsonpath={.items[0].metadata.name}"
            ], capture_output=True, text=True, check=True)
            
            pod_name = result.stdout.strip()
            if pod_name:
                print(f"Found sequencer pod: {pod_name}")
                return pod_name
            return None
            
        except subprocess.CalledProcessError as e:
            print(f"Error finding pod: {e}")
            return None
    
    def get_metrics_from_pod(self) -> Optional[str]:
        """Get metrics data from the pod"""
        if not self.pod_name:
            return None
            
        try:
            # Try to get metrics from the metrics endpoint.
            result = subprocess.run([
                "kubectl", "exec", self.pod_name, "-n", self.namespace, "--",
                "curl", "-s", "http://localhost:9090/metrics"
            ], capture_output=True, text=True, timeout=10)
            
            if result.returncode == 0:
                return result.stdout
            else:
                print(f"Metrics endpoint not available, trying alternative...")
                return None
                
        except subprocess.TimeoutExpired:
            print("Timeout getting metrics")
            return None
        except Exception as e:
            print(f"Error getting metrics: {e}")
            return None
    
    def parse_flush_metrics(self, metrics_text: str) -> Dict:
        """Parse flush latency metrics from Prometheus format"""
        metrics = {
            'timestamp': datetime.now().isoformat(),
            'flush_count': 0,
            'flush_sum': 0.0,
            'flush_avg': 0.0,
            'flush_rate': 0.0
        }
        
        # Look for the flush latency metrics.
        flush_patterns = [
            r'storage_file_handler_flush_latency_seconds_count\s+(\d+(?:\.\d+)?)',
            r'storage_file_handler_flush_latency_seconds_sum\s+(\d+(?:\.\d+)?)'
        ]
        
        for line in metrics_text.split('\n'):
            # Skip comments.
            if line.startswith('#'):
                continue
                
            # Parse count.
            count_match = re.search(flush_patterns[0], line)
            if count_match:
                metrics['flush_count'] = float(count_match.group(1))
                
            # Parse sum.
            sum_match = re.search(flush_patterns[1], line)
            if sum_match:
                metrics['flush_sum'] = float(sum_match.group(1))
        
        # Calculate average.
        if metrics['flush_count'] > 0:
            metrics['flush_avg'] = metrics['flush_sum'] / metrics['flush_count']
            
        return metrics
    
    def get_pod_logs(self, lines: int = 10) -> str:
        """Get recent pod logs."""
        if not self.pod_name:
            return "No pod found"
            
        try:
            result = subprocess.run([
                "kubectl", "logs", self.pod_name, "-n", self.namespace, 
                f"--tail={lines}"
            ], capture_output=True, text=True, check=True)
            
            return result.stdout
            
        except subprocess.CalledProcessError as e:
            return f"Error getting logs: {e}"
    
    def display_performance_summary(self, current_metrics: Dict, previous_metrics: Dict = None):
        """Display performance summary."""
        print("\n" + "="*60)
        print(f"FLUSH PERFORMANCE SUMMARY - {current_metrics['timestamp'][:19]}")
        print("="*60)
        
        print(f"Total flush operations: {current_metrics['flush_count']}")
        print(f"Total flush time: {current_metrics['flush_sum']:.6f}s")
        print(f"Average flush time: {current_metrics['flush_avg']:.6f}s ({current_metrics['flush_avg']*1000:.3f}ms)")
        
        if previous_metrics and previous_metrics['flush_count'] > 0:
            # Calculate rate since last measurement.
            count_diff = current_metrics['flush_count'] - previous_metrics['flush_count']
            time_diff = (datetime.fromisoformat(current_metrics['timestamp']) - 
                        datetime.fromisoformat(previous_metrics['timestamp'])).total_seconds()
    
    def monitor_continuous(self, interval: int = 30):
        """Monitor flush performance continuously."""
        print("Starting Continuous Flush Performance Monitoring")
        print("=" * 50)
        print(f"Namespace: {self.namespace}")
        print(f"Update interval: {interval} seconds")
        print("Press Ctrl+C to stop monitoring")
        print()
        
        previous_metrics = None
        
        try:
            while True:
                # Find pod if not found.
                if not self.pod_name:
                    self.pod_name = self.find_sequencer_pod()
                    if not self.pod_name:
                        print("Waiting for sequencer pod to be available...")
                        time.sleep(10)
                        continue
                
                # Get metrics.
                metrics_text = self.get_metrics_from_pod()
                if metrics_text:
                    current_metrics = self.parse_flush_metrics(metrics_text)
                    
                    # Display summary.
                    self.display_performance_summary(current_metrics, previous_metrics)
                    
                    # Store for next iteration.
                    previous_metrics = current_metrics
                    self.metrics_data.append(current_metrics)
                    
                else:
                    print("Could not retrieve metrics, checking pod status...")
                    # Check if pod is still running.
                    self.pod_name = self.find_sequencer_pod()
                
                # Wait for next iteration.
                print(f"\nNext update in {interval} seconds...")
                time.sleep(interval)
                
        except KeyboardInterrupt:
            print("\n\nMonitoring stopped by user")
            self.display_final_summary()
    
    def display_final_summary(self):
        """Display final performance summary."""
        if not self.metrics_data:
            print("No metrics data collected")
            return
            
        print("\n" + "="*60)
        print("FINAL PERFORMANCE SUMMARY")
        print("="*60)
        
        first_metrics = self.metrics_data[0]
        last_metrics = self.metrics_data[-1]
        
        total_operations = last_metrics['flush_count'] - first_metrics['flush_count']
        total_time = (datetime.fromisoformat(last_metrics['timestamp']) - 
                     datetime.fromisoformat(first_metrics['timestamp'])).total_seconds()
        
        if total_time > 0:
            avg_rate = total_operations / total_time
            print(f"Average flush rate: {avg_rate:.2f} operations/second")
            
        print(f"Final average flush time: {last_metrics['flush_avg']*1000:.3f}ms")
        print(f"Total operations monitored: {total_operations}")
        print(f"Monitoring duration: {total_time:.1f} seconds")

def main():
    if len(sys.argv) > 1:
        if sys.argv[1] in ['--help', '-h']:
            print("Usage: python3 monitor_flush_performance.py [namespace]")
            print("Monitor flush performance of sequencer in Kubernetes")
            print("Default namespace: sequencer")
            return
        namespace = sys.argv[1]
    else:
        namespace = "sequencer"
    
    monitor = FlushPerformanceMonitor(namespace)
    
    print("Kubernetes Flush Performance Monitor")
    print("=" * 40)
    
    # Check if kubectl is available.
    try:
        subprocess.run(["kubectl", "version", "--client"], 
                      capture_output=True, check=True)
    except (subprocess.CalledProcessError, FileNotFoundError):
        print("kubectl not found. Please install kubectl first.")
        return
    
    # Start monitoring.
    monitor.monitor_continuous()

if __name__ == "__main__":
    main()
