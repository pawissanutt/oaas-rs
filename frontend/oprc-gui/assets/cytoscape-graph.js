// Cytoscape.js integration for topology visualization
// This will be loaded via CDN in the HTML head

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

    // Convert Rust data to Cytoscape format
    const nodes = nodesData.map(node => ({
        data: {
            id: node.id,
            label: node.id,
            type: node.node_type,
            status: node.status,
            metadata: node.metadata
        },
        classes: node.status
    }));

    const edges = edgesData.map((edge, idx) => ({
        data: {
            id: `edge-${idx}`,
            source: edge[0],
            target: edge[1]
        }
    }));

    // Initialize Cytoscape
    const cy = cytoscape({
        container: document.getElementById(containerId),
        
        elements: {
            nodes: nodes,
            edges: edges
        },

        style: [
            // Node styles
            {
                selector: 'node',
                style: {
                    'background-color': '#4B5563',
                    'label': 'data(label)',
                    'color': '#fff',
                    'text-valign': 'center',
                    'text-halign': 'center',
                    'font-size': '12px',
                    'width': '60px',
                    'height': '60px',
                    'border-width': '2px',
                    'border-color': '#6B7280'
                }
            },
            // Node type-specific styles
            {
                selector: 'node[type="gateway"]',
                style: {
                    'background-color': '#3B82F6',
                    'border-color': '#2563EB',
                    'shape': 'round-rectangle'
                }
            },
            {
                selector: 'node[type="router"]',
                style: {
                    'background-color': '#8B5CF6',
                    'border-color': '#7C3AED',
                    'shape': 'diamond'
                }
            },
            {
                selector: 'node[type="odgm"]',
                style: {
                    'background-color': '#10B981',
                    'border-color': '#059669',
                    'shape': 'round-rectangle'
                }
            },
            {
                selector: 'node[type="function"]',
                style: {
                    'background-color': '#F59E0B',
                    'border-color': '#D97706',
                    'shape': 'ellipse'
                }
            },
            // Status-based colors
            {
                selector: 'node.healthy',
                style: {
                    'border-color': '#10B981',
                    'border-width': '3px'
                }
            },
            {
                selector: 'node.degraded',
                style: {
                    'border-color': '#F59E0B',
                    'border-width': '3px'
                }
            },
            {
                selector: 'node.down',
                style: {
                    'border-color': '#EF4444',
                    'border-width': '3px'
                }
            },
            // Edge styles
            {
                selector: 'edge',
                style: {
                    'width': 2,
                    'line-color': '#9CA3AF',
                    'target-arrow-color': '#9CA3AF',
                    'target-arrow-shape': 'triangle',
                    'curve-style': 'bezier',
                    'arrow-scale': 1.2
                }
            },
            // Selected/hover states
            {
                selector: 'node:selected',
                style: {
                    'border-width': '4px',
                    'border-color': '#3B82F6',
                    'background-color': '#60A5FA'
                }
            },
            {
                selector: 'edge:selected',
                style: {
                    'width': 3,
                    'line-color': '#3B82F6'
                }
            }
        ],

        layout: {
            name: 'cose',
            idealEdgeLength: 100,
            nodeOverlap: 20,
            refresh: 20,
            fit: true,
            padding: 30,
            randomize: false,
            componentSpacing: 100,
            nodeRepulsion: 400000,
            edgeElasticity: 100,
            nestingFactor: 5,
            gravity: 80,
            numIter: 1000,
            initialTemp: 200,
            coolingFactor: 0.95,
            minTemp: 1.0
        },

        // Enable interactions
        minZoom: 0.5,
        maxZoom: 2
        // wheelSensitivity: default (removed to avoid warning)
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

    // Hover effect
    cy.on('mouseover', 'node', function(evt) {
        document.body.style.cursor = 'pointer';
    });

    cy.on('mouseout', 'node', function(evt) {
        document.body.style.cursor = 'default';
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
                'color': '#F3F4F6'
            })
            .update();
    } else {
        cy.style()
            .selector('node')
            .style({
                'color': '#fff'
            })
            .update();
    }
};
