# frozen_string_literal: true

module Inkoc
  module Codegen
    class Instruction
      include Inspect

      NAME_MAPPING = %i[
        SetLiteral
        SetObject
        SetArray
        GetBuiltinPrototype
        GetTrue
        GetFalse
        SetLocal
        GetLocal
        SetBlock
        Return
        GotoIfFalse
        GotoIfTrue
        Goto
        RunBlock
        IntegerAdd
        IntegerDiv
        IntegerMul
        IntegerSub
        IntegerMod
        IntegerToFloat
        IntegerToString
        IntegerBitwiseAnd
        IntegerBitwiseOr
        IntegerBitwiseXor
        IntegerShiftLeft
        IntegerShiftRight
        IntegerSmaller
        IntegerGreater
        IntegerEquals
        FloatAdd
        FloatMul
        FloatDiv
        FloatSub
        FloatMod
        FloatToInteger
        FloatToString
        FloatSmaller
        FloatGreater
        FloatEquals
        ArraySet
        ArrayAt
        ArrayRemove
        ArrayLength
        ArrayClear
        StringToLower
        StringToUpper
        StringEquals
        StringToByteArray
        StringLength
        StringSize
        StdoutWrite
        StderrWrite
        StdinRead
        FileOpen
        FileWrite
        FileRead
        FileFlush
        FileSize
        FileSeek
        ModuleLoad
        SetAttribute
        GetAttribute
        SetPrototype
        GetPrototype
        LocalExists
        ProcessSpawn
        ProcessSendMessage
        ProcessReceiveMessage
        ProcessCurrent
        SetParentLocal
        GetParentLocal
        ObjectEquals
        GetNil
        AttributeExists
        RemoveAttribute
        GetAttributeNames
        TimeMonotonic
        GetGlobal
        SetGlobal
        Throw
        SetRegister
        TailCall
        ProcessSuspendCurrent
        IntegerGreaterOrEqual
        IntegerSmallerOrEqual
        FloatGreaterOrEqual
        FloatSmallerOrEqual
        CopyBlocks
        FloatIsNan
        FloatIsInfinite
        FloatFloor
        FloatCeil
        FloatRound
        Drop
        SetBlocking
        StdoutFlush
        StderrFlush
        FileRemove
        Panic
        Exit
        Platform
        FileCopy
        FileType
        FileTime
        TimeSystem
        TimeSystemOffset
        TimeSystemDst
        DirectoryCreate
        DirectoryRemove
        DirectoryList
        StringConcat
        HasherNew
        HasherWrite
        HasherToHash
        Stacktrace
        ProcessTerminateCurrent
        StringSlice
        BlockMetadata
        StringFormatDebug
        StringConcatMultiple
        ByteArrayFromArray
        ByteArraySet
        ByteArrayAt
        ByteArrayRemove
        ByteArrayLength
        ByteArrayClear
        ByteArrayEquals
        ByteArrayToString
        EnvGet
        EnvSet
        EnvVariables
        EnvHomeDirectory
        EnvTempDirectory
        EnvGetWorkingDirectory
        EnvSetWorkingDirectory
        EnvArguments
        EnvRemove
        BlockGetReceiver
        BlockSetReceiver
        RunBlockWithReceiver
        ProcessSetPanicHandler
        ProcessAddDeferToCaller
        SetDefaultPanicHandler
        ProcessPinThread
        ProcessUnpinThread
        LibraryOpen
        FunctionAttach
        FunctionCall
        PointerAttach
        PointerRead
        PointerWrite
        PointerFromAddress
        PointerAddress
        ForeignTypeSize
        ForeignTypeAlignment
        StringToInteger
        StringToFloat
        FloatToBits
        ProcessIdentifier
        SocketCreate
        SocketWrite
        SocketRead
        SocketAccept
        SocketReceiveFrom
        SocketSendTo
        SocketAddress
        SocketGetOption
        SocketSetOption
        SocketBind
        SocketListen
        SocketConnect
        SocketShutdown
        HasherReset
        RandomNumber
        RandomRange
        RandomBytes
        StringByte
        ModuleList
        ModuleGet
        ModuleInfo
        GetAttributeInSelf
      ]
        .each_with_index
        .each_with_object({}) { |(value, index), hash| hash[value] = index }
        .freeze

      attr_reader :index, :arguments, :location

      def self.named(name, arguments, location)
        new(NAME_MAPPING.fetch(name), arguments, location)
      end

      def initialize(index, arguments, location)
        @index = index
        @arguments = arguments
        @location = location
      end

      def line
        @location.line
      end
    end
  end
end
