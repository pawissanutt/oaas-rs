import Link from "next/link";
import {
  Package,
  Tags,
  Zap,
  Rocket,
  ArrowRight,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  Clock,
  Box,
} from "lucide-react";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";

export default function Dashboard() {
  return (
    <div className="space-y-6">
      <h1 className="text-3xl font-bold tracking-tight">Dashboard</h1>

      {/* Stat Cards */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Packages</CardTitle>
            <Package className="h-4 w-4 text-primary" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">12</div>
            <p className="text-xs text-muted-foreground">
              +2 from last week
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Classes</CardTitle>
            <Tags className="h-4 w-4 text-purple-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">34</div>
            <p className="text-xs text-muted-foreground">
              Across 12 packages
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Functions</CardTitle>
            <Zap className="h-4 w-4 text-amber-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">56</div>
            <p className="text-xs text-muted-foreground">
              4 macros, 52 custom
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Deployments</CardTitle>
            <Rocket className="h-4 w-4 text-emerald-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">8</div>
            <p className="text-xs text-muted-foreground">
              5 running, 2 pending
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
                <div className="text-sm text-muted-foreground">5</div>
              </div>
              <div className="flex items-center">
                <div className="h-2 w-2 rounded-full bg-yellow-500 mr-2" />
                <div className="flex-1 text-sm font-medium">Pending</div>
                <div className="text-sm text-muted-foreground">2</div>
              </div>
              <div className="flex items-center">
                <div className="h-2 w-2 rounded-full bg-red-500 mr-2" />
                <div className="flex-1 text-sm font-medium">Error</div>
                <div className="text-sm text-muted-foreground">1</div>
              </div>
            </div>
            <div className="space-y-2">
              <div className="flex items-center justify-between text-sm">
                <span className="text-muted-foreground">Health Score</span>
                <span className="font-medium">62%</span>
              </div>
              <Progress value={62} className="h-2" />
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
                <Badge variant="success">Healthy</Badge>
              </div>
              <span className="text-xs text-muted-foreground flex items-center">
                <Clock className="w-3 h-3 mr-1" />
                Updated 5m ago
              </span>
            </div>
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium flex items-center">
                  <div className="w-2 h-2 rounded-full bg-green-500 mr-2 animate-pulse" />
                  cluster-1
                </span>
                <Badge variant="outline" className="text-xs font-normal">Sensitive</Badge>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium flex items-center">
                  <div className="w-2 h-2 rounded-full bg-yellow-500 mr-2" />
                  cluster-2
                </span>
                <Badge variant="secondary" className="text-xs font-normal">Degraded</Badge>
              </div>
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
            <Link href="/deployments">
              <div className="flex w-full items-center justify-between">
                <Rocket className="h-5 w-5 mb-2" />
                <ArrowRight className="h-4 w-4 text-muted-foreground" />
              </div>
              <span className="font-medium">Deployments</span>
              <span className="text-xs text-muted-foreground font-normal text-left">
                Check deployment status
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
