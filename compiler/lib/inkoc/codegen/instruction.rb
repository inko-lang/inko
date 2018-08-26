# frozen_string_literal: true

module Inkoc
  module Codegen
    class Instruction
      include Inspect

      NAME_MAPPING = %i[
        SetLiteral
        SetObject
        SetArray
        GetIntegerPrototype
        GetFloatPrototype
        GetStringPrototype
        GetArrayPrototype
        GetBlockPrototype
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
        LoadModule
        SetAttribute
        GetAttribute
        SetPrototype
        GetPrototype
        LocalExists
        ProcessSpawn
        ProcessSendMessage
        ProcessReceiveMessage
        ProcessCurrentPid
        SetParentLocal
        GetParentLocal
        ObjectEquals
        GetToplevel
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
        ProcessStatus
        ProcessSuspendCurrent
        IntegerGreaterOrEqual
        IntegerSmallerOrEqual
        FloatGreaterOrEqual
        FloatSmallerOrEqual
        ObjectIsKindOf
        CopyBlocks
        GetObjectPrototype
        SetAttributeToObject
        PrototypeChainAttributeContains
        FloatIsNan
        FloatIsInfinite
        FloatFloor
        FloatCeil
        FloatRound
        Drop
        MoveToPool
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
        HasherFinish
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
        GetBooleanPrototype
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
