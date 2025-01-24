import matplotlib.pyplot as plt
import json

# Initialize lists to store data
x = []
y = []

# Read and parse the JSON lines
with open("data_points.ndjson", "r") as file:
    for line in file:
        data = json.loads(line)
        request_rate = data["request_rate"]
        mean_latency = data["result"]["latency"]["stats"]["mean"]
        
        x.append(request_rate)
        y.append(mean_latency)

# Plot the actual data
plt.plot(x, y, label='Actual Data', color='blue', marker='o')

# Generate dummy data for comparison
x_dummy = [1000, 1200, 1400, 1600, 1800, 2000]
y_dummy1 = [0.001, 0.0012, 0.0014, 0.0011, 0.0013, 0.0015]
y_dummy2 = [0.0009, 0.0011, 0.0012, 0.0010, 0.0011, 0.0014]

# Plot dummy data with different colors
plt.plot(x_dummy, y_dummy1, label='Dummy Data 1', color='red', linestyle='--', marker='x')
plt.plot(x_dummy, y_dummy2, label='Dummy Data 2', color='green', linestyle=':', marker='s')

# Labels and title
plt.xlabel('Request Rate')
plt.ylabel('Latency (seconds)')
plt.title('Latency vs Request Rate Comparison')
plt.legend()

# Show the plot
plt.show()
