"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import {
  Package,
  Tags,
  Zap,
  Rocket,
  ArrowRight,
  Clock,
  Box,
  Loader2,
} from "lucide-react";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { fetchPackages, fetchDeployments, fetchEnvironments, fetchHealth, HealthResponse } from "@/lib/api";
import { OPackage } from "@/lib/bindings/OPackage";
import { OClassDeployment } from "@/lib/bindings/OClassDeployment";
import { ClusterInfo } from "@/lib/types";

export default function Dashboard() {
  const [stats, setStats] = useState({
    packages: 0,
    classes: 0,
    functions: 0,
    deployments: 0,
    running: 0,
    pending: 0,
    error: 0,
  });
  const [envs, setEnvs] = useState<ClusterInfo[]>([]);
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function loadData() {
      try {
        const [pkgs, deps, environments, healthData] = await Promise.all([
          fetchPackages(),
          fetchDeployments(),
          fetchEnvironments(),
          fetchHealth().catch(() => null),
        ]);

        setHealth(healthData);

        let classCount = 0;
        let fnCount = 0;
        pkgs.forEach(p => {
          classCount += p.classes.length;
          fnCount += p.functions.length;
        });

        // Mock deployment status calculation since OClassDeployment status is complex or null
        // We can check if status object exists.
        const running = deps.filter(d => d.condition === "RUNNING").length;
        const pending = deps.filter(d => d.condition === "PENDING" || d.condition === "DEPLOYING").length;
        const error = deps.filter(d => d.condition === "DOWN" || d.condition === "DELETED").length; // Approximate error/down

        setStats({
          packages: pkgs.length,
          classes: classCount,
          functions: fnCount,
          deployments: deps.length,
          running,
          pending,
          error
        });

        setEnvs(environments);
      } catch (e) {
        console.error(e);
      } finally {
        setLoading(false);
      }
    }
    loadData();
  }, []);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold tracking-tight">Dashboard</h1>
        {loading && <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />}
      </div>

      {/* Stat Cards */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Packages</CardTitle>
            <Package className="h-4 w-4 text-primary" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.packages}</div>
            <p className="text-xs text-muted-foreground">
              Total uploaded packages
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Classes</CardTitle>
            <Tags className="h-4 w-4 text-purple-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.classes}</div>
            <p className="text-xs text-muted-foreground">
              Defined across packages
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Functions</CardTitle>
            <Zap className="h-4 w-4 text-amber-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.functions}</div>
            <p className="text-xs text-muted-foreground">
              Executable functions
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Deployments</CardTitle>
            <Rocket className="h-4 w-4 text-emerald-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.deployments}</div>
            <p className="text-xs text-muted-foreground">
              Active class deployments
            </p>
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-7">
        {/* Deployment Status */}
        <Card className="col-span-4">
          <CardHeader>
            <CardTitle>Deployment Status</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid gap-4">
              <div className="flex items-center">
                <div className="h-2 w-2 rounded-full bg-green-500 mr-2" />
                <div className="flex-1 text-sm font-medium">Running</div>
                <div className="text-sm text-muted-foreground">{stats.running}</div>
              </div>
              <div className="flex items-center">
                <div className="h-2 w-2 rounded-full bg-yellow-500 mr-2" />
                <div className="flex-1 text-sm font-medium">Pending</div>
                <div className="text-sm text-muted-foreground">{stats.pending}</div>
              </div>
              {stats.error > 0 && (
                <div className="flex items-center">
                  <div className="h-2 w-2 rounded-full bg-red-500 mr-2" />
                  <div className="flex-1 text-sm font-medium">Error</div>
                  <div className="text-sm text-muted-foreground">{stats.error}</div>
                </div>
              )}
            </div>
            <div className="space-y-2">
              <div className="flex items-center justify-between text-sm">
                <span className="text-muted-foreground">Health Score</span>
                <span className="font-medium">
                  {stats.deployments > 0 ? Math.round((stats.running / stats.deployments) * 100) : 100}%
                </span>
              </div>
              <Progress
                value={stats.deployments > 0 ? (stats.running / stats.deployments) * 100 : 100}
                className="h-2"
              />
            </div>
          </CardContent>
        </Card>

        {/* System Health */}
        <Card className="col-span-3">
          <CardHeader>
            <CardTitle>System Health</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center justify-between mb-6">
              <div className="flex items-center space-x-2">
                {health ? (
                  <Badge variant={health.status === "healthy" ? "success" : "destructive"}>
                    {health.status === "healthy" ? "Healthy" : "Unhealthy"}
                  </Badge>
                ) : (
                  <Badge variant="secondary">Unknown</Badge>
                )}
              </div>
              <span className="text-xs text-muted-foreground flex items-center">
                <Clock className="w-3 h-3 mr-1" />
                {health?.timestamp ? new Date(health.timestamp).toLocaleTimeString() : "--"}
              </span>
            </div>
            {health && (
              <div className="text-xs text-muted-foreground mb-4">
                {health.service} v{health.version} &middot; Storage: {health.storage.status}
              </div>
            )}
            <div className="space-y-4">
              {envs.length === 0 ? (
                <div className="text-sm text-muted-foreground">No environments detected.</div>
              ) : (
                envs.map(env => (
                  <div key={env.name} className="flex items-center justify-between">
                    <span className="text-sm font-medium flex items-center">
                      <div className={`w-2 h-2 rounded-full mr-2 ${env.status === 'Healthy' ? 'bg-green-500' : 'bg-yellow-500'}`} />
                      {env.name}
                    </span>
                    <Badge variant={env.status === 'Healthy' ? "outline" : "secondary"} className="text-xs font-normal">
                      {env.status}
                    </Badge>
                  </div>
                ))
              )}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Quick Actions */}
      <div>
        <h2 className="text-lg font-semibold mb-4">Quick Actions</h2>
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4">
          <Button variant="outline" className="h-auto p-4 flex flex-col items-start gap-2" asChild>
            <Link href="/packages">
              <div className="flex w-full items-center justify-between">
                <Package className="h-5 w-5 mb-2" />
                <ArrowRight className="h-4 w-4 text-muted-foreground" />
              </div>
              <span className="font-medium">Manage Packages</span>
              <span className="text-xs text-muted-foreground font-normal text-left">
                Create or update packages
              </span>
            </Link>
          </Button>

          <Button variant="outline" className="h-auto p-4 flex flex-col items-start gap-2" asChild>
            <Link href="/topology">
              <div className="flex w-full items-center justify-between">
                <Rocket className="h-5 w-5 mb-2" />
                <ArrowRight className="h-4 w-4 text-muted-foreground" />
              </div>
              <span className="font-medium">View Topology</span>
              <span className="text-xs text-muted-foreground font-normal text-left">
                Inspect system topology
              </span>
            </Link>
          </Button>

          <Button variant="outline" className="h-auto p-4 flex flex-col items-start gap-2" asChild>
            <Link href="/objects">
              <div className="flex w-full items-center justify-between">
                <Box className="h-5 w-5 mb-2" />
                <ArrowRight className="h-4 w-4 text-muted-foreground" />
              </div>
              <span className="font-medium">Browse Objects</span>
              <span className="text-xs text-muted-foreground font-normal text-left">
                View stored objects
              </span>
            </Link>
          </Button>
        </div>
      </div>
    </div>
  );
}
