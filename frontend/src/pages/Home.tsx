import { Link } from 'react-router'
import { LogIn, Server } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'

export function Home() {
  return (
    <div className="flex min-h-svh items-center justify-center bg-background p-6">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Server className="size-5" />
            iPXE ZTP Server
          </CardTitle>
          <CardDescription>
            Zero-touch provisioning — machines PXE-boot into iPXE and get an OS
            installed automatically.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex gap-2">
          <Button render={<Link to="/login" />} size="lg" className="w-full">
            <LogIn data-icon="inline-start" />
            Log in
          </Button>
        </CardContent>
      </Card>
    </div>
  )
}
