@{
    # Extract = the one file taken from the package. System.Management 10.0.2 is also a dependency, but
    # on .NET Framework its package is a placeholder (lib/net462/_._): the framework's own assembly is used.
    NuGet = @(
        @{ Id = 'LibreHardwareMonitorLib'; Version = '0.9.6'; License = 'MPL-2.0'
           Sha256 = '60db5e0d9135f4658b385cae139d3b9eaeac0a8e867d322d20b5dbe4c4039197'
           Extract = 'runtimes/win-x64/lib/net472/LibreHardwareMonitorLib.dll' }
        @{ Id = 'DiskInfoToolkit'; Version = '1.1.2'; License = 'MPL-2.0'
           Sha256 = 'a997f0b3701e20b2ab9296e07ee4f74f877e75015277fb6bd314ade1e2c4db13'
           Extract = 'lib/net472/DiskInfoToolkit.dll' }
        @{ Id = 'RAMSPDToolkit-NDD'; Version = '1.4.2'; License = 'MPL-2.0'
           Sha256 = '57c893824a68c71538055f242586616d4205a671c300897c21b2a6f88270ce64'
           Extract = 'lib/net472/RAMSPDToolkit-NDD.dll' }
        @{ Id = 'BlackSharp.Core'; Version = '1.0.7'; License = 'MPL-2.0'
           Sha256 = '4f713fe18ee7c85a7c49f28309adaa815714333326aa81fd4d4a81619ecaa0ab'
           Extract = 'lib/net472/BlackSharp.Core.dll' }
        @{ Id = 'HidSharp'; Version = '2.6.4'; License = 'Apache-2.0'
           Sha256 = 'cf55d4b4d7b42dc1d8496b7b592ba00d1ecd48a70bc07bd8dabd3e9976d108e7'
           Extract = 'lib/netstandard2.0/HidSharp.dll' }
        @{ Id = 'System.Memory'; Version = '4.6.3'; License = 'MIT'
           Sha256 = '26078aeb758c9ae985e8bf851f973026061da6a5eb4837204d0c2d2204c72955'
           Extract = 'lib/net462/System.Memory.dll' }
        @{ Id = 'System.Buffers'; Version = '4.6.1'; License = 'MIT'
           Sha256 = 'b00451e91d016fbec091ad1e361f3a7015e1d91d4047f7e48a74455b2a673d79'
           Extract = 'lib/net462/System.Buffers.dll' }
        @{ Id = 'System.Numerics.Vectors'; Version = '4.6.1'; License = 'MIT'
           Sha256 = '2bc500a86dcb02f2032d6d877f9e2d6e9e4a79080e57239b4198679d4031f2c7'
           Extract = 'lib/net462/System.Numerics.Vectors.dll' }
        @{ Id = 'System.Runtime.CompilerServices.Unsafe'; Version = '6.1.2'; License = 'MIT'
           Sha256 = '5f6a7f53af3465f92beb6da873ebe0e496206c313313b98badee4355a6b25937'
           Extract = 'lib/net462/System.Runtime.CompilerServices.Unsafe.dll' }
        @{ Id = 'System.Threading.AccessControl'; Version = '10.0.3'; License = 'MIT'
           Sha256 = '9f4f28238e13c6cc47b789a4a1d5647997747594dfcfd45c1526f9b671cde073'
           Extract = 'lib/net462/System.Threading.AccessControl.dll' }
        @{ Id = 'System.Security.AccessControl'; Version = '6.0.0'; License = 'MIT'
           Sha256 = 'a8ec961016cdaf7123c92f9eb451bcff331bb8f8c0f8ef9d8bbd7b24ff42c728'
           Extract = 'lib/net461/System.Security.AccessControl.dll' }
        @{ Id = 'System.Security.Principal.Windows'; Version = '5.0.0'; License = 'MIT'
           Sha256 = '081390c25f6f78592b28ada853c24514488a221fe9f9a24efaaf5373643ff3d6'
           Extract = 'lib/net461/System.Security.Principal.Windows.dll' }
    )

    # The official signed driver installer. Signer: CN=namazso.eu, issued by GLOBALTRUST 2015 CODESIGNING 1.
    PawnIO = @{
        Version = '2.2.0'
        Url = 'https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe'
        Sha256 = '1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032'
        SignerThumbprint = 'F380DCC9F706E2756A5047B832FFE719E1BC35F5'
        SignerSubjectContains = 'CN=namazso.eu'
    }

}

