// Cytoscape.js integration for topology visualization
// This will be loaded via CDN in the HTML head

// Helper to create a shorter display label from node ID
function getDisplayLabel(nodeId, nodeType) {
    // Remove prefix like "router::", "function::", etc.
    const parts = nodeId.split('::');
    if (parts.length > 1) {
        const name = parts.slice(1).join('::');
        // For long hex IDs (like router ZIDs), show shortened version
        if (name.length === 32 && /^[a-f0-9]+$/i.test(name)) {
            return name.substring(0, 8) + '…';
        }
        // For partition/replica, show compact form
        if (nodeType === 'partition') {
            return 'P' + parts[parts.length - 1];
        }
        if (nodeType === 'replica') {
            return 'R' + parts[parts.length - 1];
        }
        // For function, show just the function name
        if (nodeType === 'function' && parts.length >= 2) {
            return parts[parts.length - 1];
        }
        return name;
    }
    return nodeId;
}

window.initCytoscapeGraph = function(containerId, nodesData, edgesData) {
    // Check if Cytoscape is loaded
    if (typeof cytoscape === 'undefined') {
        console.error('Cytoscape.js library not loaded');
        return null;
    }

    // Check if container exists
    const container = document.getElementById(containerId);
    if (!container) {
        console.error('Container not found:', containerId);
        return null;
    }

    // Convert Rust data to Cytoscape format with better labels
    const nodes = nodesData.map(node => ({
        data: {
            id: node.id,
            label: getDisplayLabel(node.id, node.node_type),
            type: node.node_type,
            status: node.status,
            metadata: node.metadata
        },
        classes: [node.status, node.node_type].join(' ')
    }));

    const edges = edgesData.map((edge, idx) => ({
        data: {
            id: `edge-${idx}`,
            source: edge.from_id,
            target: edge.to_id
        }
    }));

    // Initialize Cytoscape with modern styling
    const cy = cytoscape({
        container: document.getElementById(containerId),
        
        elements: {
            nodes: nodes,
            edges: edges
        },

        style: [
            // Base node styles - clean modern look
            {
                selector: 'node',
                style: {
                    'label': 'data(label)',
                    'text-valign': 'bottom',
                    'text-halign': 'center',
                    'text-margin-y': 8,
                    'font-size': '11px',
                    'font-weight': '500',
                    'font-family': 'Inter, system-ui, sans-serif',
                    'color': '#E5E7EB',
                    'text-outline-color': '#1F2937',
                    'text-outline-width': 2,
                    'width': 50,
                    'height': 50,
                    'background-color': '#4B5563',
                    'border-width': 3,
                    'border-color': '#6B7280',
                    'border-opacity': 1,
                    'background-opacity': 0.95,
                    'overlay-opacity': 0,
                    'shadow-blur': 8,
                    'shadow-color': '#000',
                    'shadow-opacity': 0.3,
                    'shadow-offset-x': 0,
                    'shadow-offset-y': 2
                }
            },
            
            // Package - Large rounded rectangle (indigo)
            {
                selector: 'node[type="package"]',
                style: {
                    'shape': 'round-rectangle',
                    'width': 90,
                    'height': 45,
                    'background-color': '#6366F1',
                    'border-color': '#818CF8',
                    'border-width': 2,
                    'text-valign': 'center',
                    'text-halign': 'center',
                    'text-margin-y': 0,
                    'font-size': '12px',
                    'font-weight': '600'
                }
            },
            
            // Class - Circle (teal/cyan - distinct from ODGM green)
            {
                selector: 'node[type="class"]',
                style: {
                    'shape': 'ellipse',
                    'width': 55,
                    'height': 55,
                    'background-color': '#06B6D4',
                    'border-color': '#22D3EE',
                    'border-width': 3
                }
            },
            
            // Function - Rounded pill (amber/orange)
            {
                selector: 'node[type="function"]',
                style: {
                    'shape': 'round-rectangle',
                    'width': 70,
                    'height': 35,
                    'background-color': '#F59E0B',
                    'border-color': '#FBBF24',
                    'border-width': 2,
                    'text-valign': 'center',
                    'text-halign': 'center',
                    'text-margin-y': 0,
                    'font-size': '11px'
                }
            },
            
            // Environment - Hexagon (pink/magenta)
            {
                selector: 'node[type="environment"]',
                style: {
                    'shape': 'hexagon',
                    'width': 65,
                    'height': 65,
                    'background-color': '#EC4899',
                    'border-color': '#F472B6',
                    'border-width': 3,
                    'font-size': '10px'
                }
            },
            
            // Router - Diamond (purple)
            {
                selector: 'node[type="router"]',
                style: {
                    'shape': 'diamond',
                    'width': 55,
                    'height': 55,
                    'background-color': '#8B5CF6',
                    'border-color': '#A78BFA',
                    'border-width': 3
                }
            },
            
            // Gateway - Rounded rectangle (blue)
            {
                selector: 'node[type="gateway"]',
                style: {
                    'shape': 'round-rectangle',
                    'width': 70,
                    'height': 40,
                    'background-color': '#3B82F6',
                    'border-color': '#60A5FA',
                    'border-width': 2,
                    'text-valign': 'center',
                    'text-halign': 'center',
                    'text-margin-y': 0
                }
            },
            
            // ODGM - Barrel/database shape (green - distinct from class teal)
            {
                selector: 'node[type="odgm"]',
                style: {
                    'shape': 'barrel',
                    'width': 50,
                    'height': 55,
                    'background-color': '#10B981',
                    'border-color': '#34D399',
                    'border-width': 2
                }
            },
            
            // Partition - Small rounded square (yellow)
            {
                selector: 'node[type="partition"]',
                style: {
                    'shape': 'round-rectangle',
                    'width': 40,
                    'height': 40,
                    'background-color': '#EAB308',
                    'border-color': '#FACC15',
                    'border-width': 2,
                    'font-size': '10px',
                    'text-valign': 'center',
                    'text-halign': 'center',
                    'text-margin-y': 0
                }
            },
            
            // Replica - Small circle (emerald)
            {
                selector: 'node[type="replica"]',
                style: {
                    'shape': 'ellipse',
                    'width': 32,
                    'height': 32,
                    'background-color': '#059669',
                    'border-color': '#10B981',
                    'border-width': 2,
                    'font-size': '9px',
                    'text-valign': 'center',
                    'text-halign': 'center',
                    'text-margin-y': 0
                }
            },
            
            // Status-based border colors
            {
                selector: 'node.healthy',
                style: {
                    'border-color': '#10B981',
                    'border-width': 3
                }
            },
            {
                selector: 'node.degraded',
                style: {
                    'border-color': '#F59E0B',
                    'border-width': 3
                }
            },
            {
                selector: 'node.down',
                style: {
                    'border-color': '#EF4444',
                    'border-width': 3
                }
            },
            
            // Edge styles - clean with subtle arrows
            {
                selector: 'edge',
                style: {
                    'width': 2,
                    'line-color': '#6B7280',
                    'line-opacity': 0.6,
                    'target-arrow-color': '#6B7280',
                    'target-arrow-shape': 'triangle',
                    'arrow-scale': 0.8,
                    'curve-style': 'bezier',
                    'control-point-step-size': 40
                }
            },
            
            // Edges from packages (indigo)
            {
                selector: 'edge[source ^= "package::"]',
                style: {
                    'line-color': '#818CF8',
                    'target-arrow-color': '#818CF8',
                    'line-opacity': 0.7
                }
            },
            
            // Edges to functions (dashed amber)
            {
                selector: 'edge[target ^= "function::"]',
                style: {
                    'line-color': '#FBBF24',
                    'target-arrow-color': '#FBBF24',
                    'line-opacity': 0.7,
                    'line-style': 'dashed'
                }
            },
            
            // Edges to environments (pink)
            {
                selector: 'edge[target ^= "env::"]',
                style: {
                    'line-color': '#F472B6',
                    'target-arrow-color': '#F472B6',
                    'line-opacity': 0.7
                }
            },
            
            // Edges to partitions (yellow, thinner)
            {
                selector: 'edge[target ^= "partition::"]',
                style: {
                    'line-color': '#FACC15',
                    'target-arrow-color': '#FACC15',
                    'line-opacity': 0.5,
                    'width': 1.5
                }
            },
            
            // Edges to replicas (emerald, thinnest)
            {
                selector: 'edge[target ^= "replica::"]',
                style: {
                    'line-color': '#10B981',
                    'target-arrow-color': '#10B981',
                    'line-opacity': 0.5,
                    'width': 1
                }
            },
            
            // Selected state - glowing effect
            {
                selector: 'node:selected',
                style: {
                    'border-width': 4,
                    'border-color': '#FFFFFF',
                    'shadow-blur': 15,
                    'shadow-color': '#3B82F6',
                    'shadow-opacity': 0.8
                }
            },
            
            // Highlighted nodes (neighbors of selected)
            {
                selector: 'node.highlighted',
                style: {
                    'border-width': 3,
                    'border-color': '#FFFFFF',
                    'border-opacity': 0.8
                }
            },
            
            {
                selector: 'edge:selected',
                style: {
                    'width': 3,
                    'line-color': '#60A5FA',
                    'target-arrow-color': '#60A5FA',
                    'line-opacity': 1
                }
            },
            
            {
                selector: 'edge.highlighted',
                style: {
                    'width': 2.5,
                    'line-opacity': 0.9
                }
            }
        ],

        layout: {
            name: 'cose',
            idealEdgeLength: 80,
            nodeOverlap: 30,
            refresh: 20,
            fit: true,
            padding: 40,
            randomize: false,
            componentSpacing: 120,
            nodeRepulsion: 500000,
            edgeElasticity: 100,
            nestingFactor: 5,
            gravity: 100,
            numIter: 1500,
            initialTemp: 250,
            coolingFactor: 0.95,
            minTemp: 1.0
        },

        // Enable interactions
        minZoom: 0.3,
        maxZoom: 3
    });

    // Preserve any previously registered callback from Rust (do NOT overwrite)
    if (typeof window.cytoscapeNodeSelectCallback === 'undefined') {
        window.cytoscapeNodeSelectCallback = null;
    }
    
    // Add interactivity
    cy.on('tap', 'node', function(evt) {
        const node = evt.target;
        const data = node.data();
        
        // Highlight connected nodes
        cy.elements().removeClass('highlighted');
        node.neighborhood().addClass('highlighted');
        node.addClass('highlighted');
        
        // Call Rust callback if registered
        if (typeof window.cytoscapeNodeSelectCallback === 'function') {
            try {
                window.cytoscapeNodeSelectCallback(data.id);
            } catch (e) {
                console.error('Error calling callback:', e);
            }
        } else {
            console.warn('No callback registered, selected:', data.id);
        }
    });

    cy.on('tap', function(evt) {
        if (evt.target === cy) {
            cy.elements().removeClass('highlighted');
            // Clear selection in Rust
            if (window.cytoscapeNodeSelectCallback) {
                window.cytoscapeNodeSelectCallback(null);
            }
        }
    });

    // Hover effects
    cy.on('mouseover', 'node', function(evt) {
        document.body.style.cursor = 'pointer';
        evt.target.style({
            'shadow-blur': 12,
            'shadow-opacity': 0.5
        });
    });

    cy.on('mouseout', 'node', function(evt) {
        document.body.style.cursor = 'default';
        evt.target.style({
            'shadow-blur': 8,
            'shadow-opacity': 0.3
        });
    });

    // Store instance for later access
    window.cytoscapeInstance = cy;
    
    return cy;
};

// Dark mode support
window.setCytoscapeDarkMode = function(isDark) {
    const cy = window.cytoscapeInstance;
    if (!cy) return;

    if (isDark) {
        cy.style()
            .selector('node')
            .style({
                'color': '#F3F4F6',
                'text-outline-color': '#1F2937'
            })
            .update();
    } else {
        cy.style()
            .selector('node')
            .style({
                'color': '#1F2937',
                'text-outline-color': '#FFFFFF'
            })
            .update();
    }
};
