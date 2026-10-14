// CPU-only LocalSystem service. Its only IPC capability is publishing measurements.
// C# 5 / .NET Framework 4.7.2: buildable with the compiler included in Windows.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Security.Principal;
using System.Text;
using System.Threading;
using LibreHardwareMonitor.Hardware;

namespace ArgusCpuTemperature
{
    internal sealed class CpuSource : IDisposable
    {
        private Computer computer;
        private IHardware cpu;
        private Mutex pci;
        private bool usable;
        private static long Now() { return (long)(DateTime.UtcNow - new DateTime(1970, 1, 1, 0, 0, 0, DateTimeKind.Utc)).TotalMilliseconds; }

        public void Dispose()
        {
            if (pci != null) { pci.Dispose(); pci = null; }
            if (computer != null) { try { computer.Close(); } catch { } computer = null; }
            cpu = null;
            usable = false;
        }

        private static string DeviceState()
        {
            // LHM caches the registry version in a static constructor. Read it ourselves so a
            // helper started before activation can recover without keeping that initial absence.
            using (Microsoft.Win32.RegistryKey key = Microsoft.Win32.Registry.LocalMachine.OpenSubKey(@"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO"))
            {
                if (key == null) return "driver_not_installed";
                Version version;
                if (!Version.TryParse(key.GetValue("DisplayVersion") as string, out version) || version < new Version(2, 0)) return "driver_old";
            }
            IntPtr h = CreateFileW(@"\\?\GLOBALROOT\Device\PawnIO", 0xC0000000u, 3, IntPtr.Zero, 3, 0x80, IntPtr.Zero);
            if (h != new IntPtr(-1)) { CloseHandle(h); return "ok"; }
            int error = Marshal.GetLastWin32Error();
            if (error == 5) return "access_denied";
            if (error == 2 || error == 3) return "driver_not_running";
            return "driver_blocked";
        }

        private void Open()
        {
            Dispose();
            computer = new Computer();
            computer.IsCpuEnabled = true;
            computer.Open();
            foreach (IHardware h in computer.Hardware)
                if (h.HardwareType == HardwareType.Cpu) { cpu = h; break; }
            // Computer.Open creates LHM's shared PCI mutex. Failure to acquire it is fail-closed.
            pci = Mutex.OpenExisting(@"Global\Access_PCI");
            usable = true;
        }

        private static ISensor Select(IHardware hardware, SensorType type, string[] names)
        {
            foreach (string name in names)
                foreach (ISensor sensor in hardware.Sensors)
                    if (sensor.SensorType == type && sensor.Name == name) return sensor;
            return null;
        }

        private static ISensor Temperature(IHardware hardware)
        {
            // Prefer the physical die/package over an offset control temperature. Only Zen 2
            // Core (Tctl/Tdie) has been hardware-verified in Argus.
            string[] names = { "CPU Package", "Package", "Core (Tdie)", "Core (Tctl/Tdie)", "Core (Tctl)" };
            return Select(hardware, SensorType.Temperature, names);
        }

        private static string Metric(ISensor sensor, double minimum, double maximum)
        {
            if (sensor == null) return "no_sensor,";
            if (!sensor.Value.HasValue) return "no_value,";
            double value = sensor.Value.Value;
            if (double.IsNaN(value) || double.IsInfinity(value) || value < minimum || value > maximum) return "invalid,";
            return "ok," + value.ToString("R", CultureInfo.InvariantCulture);
        }

        public string Read()
        {
            string status = "error";
            string temperature = null, power = null, vcore = null;
            try
            {
                status = DeviceState();
                if (status != "ok") Dispose();
                else
                {
                    if (!usable) Open(); // Recreate driver/module handles after any device outage.
                    if (cpu == null) status = "no_sensor";
                    else
                    {
                        bool owned = false;
                        try
                        {
                            try { owned = pci.WaitOne(750); } catch (AbandonedMutexException) { owned = true; }
                            if (!owned) status = "busy";
                            else
                            {
                                // Same thread, reentrant with LHM's internal 10 ms acquisition.
                                // No ownership means no Update and no cached value is published.
                                cpu.Update();
                                temperature = Metric(Temperature(cpu), 1, 150);
                                power = Metric(Select(cpu, SensorType.Power, new[] { "CPU Package", "Package" }), 0.01, 2000);
                                // TFN is measured rail telemetry. Per-core VID is a requested
                                // voltage and must never be presented as physical Vcore.
                                vcore = Metric(Select(cpu, SensorType.Voltage, new[] { "Core (SVI2 TFN)", "Core (SVI3 TFN)" }), 0.1, 2.5);
                                // Device removal during Update must not turn a retained LHM value into Ok.
                                string after = DeviceState();
                                if (after != "ok") { status = after; DisposeAfterRead = true; }
                            }
                        }
                        finally { if (owned) pci.ReleaseMutex(); }
                        if (DisposeAfterRead) { DisposeAfterRead = false; Dispose(); }
                    }
                }
            }
            catch { status = "error"; Dispose(); }
            if (status != "ok") temperature = power = vcore = status + ",";
            return Now().ToString(CultureInfo.InvariantCulture) + "," + temperature + "," + power + "," + vcore + "\n";
        }
        private bool DisposeAfterRead;
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateFileW(string name, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
        [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr handle);
    }

    internal sealed class TemperatureService : System.ServiceProcess.ServiceBase
    {
        private volatile bool stopping;
        public TemperatureService() { ServiceName = "ArgusCpuTemperature"; CanStop = true; AutoLog = false; }
        protected override void OnStart(string[] args)
        {
            // SCM start arguments are deliberately ignored; there are no privileged commands.
            new Thread(delegate() { try { Publish(); } catch { } finally { if (!stopping) Stop(); } }) { IsBackground = true }.Start();
        }
        protected override void OnStop() { stopping = true; }
        private void Publish()
        {
            PipeSecurity security = new PipeSecurity();
            security.SetAccessRuleProtection(true, false);
            security.AddAccessRule(new PipeAccessRule(new SecurityIdentifier(WellKnownSidType.LocalSystemSid, null), PipeAccessRights.FullControl, AccessControlType.Allow));
            security.AddAccessRule(new PipeAccessRule(new SecurityIdentifier(WellKnownSidType.BuiltinAdministratorsSid, null), PipeAccessRights.FullControl, AccessControlType.Allow));
            security.AddAccessRule(new PipeAccessRule(new SecurityIdentifier(WellKnownSidType.InteractiveSid, null), PipeAccessRights.Read | PipeAccessRights.Synchronize, AccessControlType.Allow));
            List<NamedPipeServerStream> clients = new List<NamedPipeServerStream>();
            new Thread(delegate()
            {
                while (!stopping)
                {
                    NamedPipeServerStream pipe = null;
                    try
                    {
                        pipe = new NamedPipeServerStream("argus-cpu-temperature-v1", PipeDirection.Out, 8, PipeTransmissionMode.Byte, PipeOptions.Asynchronous, 0, 4096, security);
                        pipe.WaitForConnection();
                        lock (clients) { if (!stopping) clients.Add(pipe); else pipe.Dispose(); }
                    }
                    catch { if (pipe != null) pipe.Dispose(); Thread.Sleep(200); }
                }
            }) { IsBackground = true }.Start();
            using (CpuSource source = new CpuSource())
            {
                Stopwatch clock = Stopwatch.StartNew();
                long lastClient = 0;
                while (!stopping)
                {
                    long began = clock.ElapsedMilliseconds;
                    byte[] line = Encoding.ASCII.GetBytes(source.Read());
                    lock (clients)
                    {
                        for (int i = clients.Count - 1; i >= 0; i--)
                        {
                            bool sent = false;
                            try { sent = clients[i].WriteAsync(line, 0, line.Length).Wait(100); } catch { }
                            if (!sent) { clients[i].Dispose(); clients.RemoveAt(i); }
                        }
                        if (clients.Count > 0) lastClient = clock.ElapsedMilliseconds;
                    }
                    if (clock.ElapsedMilliseconds - lastClient > 30 * 1000) break;
                    Thread.Sleep((int)Math.Max(0, 1000 - (clock.ElapsedMilliseconds - began)));
                }
            }
            Stop();
            lock (clients) foreach (NamedPipeServerStream pipe in clients) pipe.Dispose();
        }
        private static int Main(string[] args)
        {
            // A fixed service executable: no probe, file paths, shell, pipe name or driver requests.
            if (args.Length != 0) return 64;
            System.ServiceProcess.ServiceBase.Run(new TemperatureService());
            return 0;
        }
    }
}
