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
        GetBooleanPrototype
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
        ArrayInsert
        ArrayAt
        ArrayRemove
        ArrayLength
        ArrayClear
        StringToLower
        StringToUpper
        StringEquals
        StringToBytes
        StringFromBytes
        StringLength
        StringSize
        StdoutWrite
        StderrWrite
        StdinRead
        StdinReadLine
        FileOpen
        FileWrite
        FileRead
        FileReadLine
        FileFlush
        FileSize
        FileSeek
        LoadModule
        GetBindingPrototype
        GetBinding
        SetAttribute
        GetAttribute
        SetPrototype
        GetPrototype
        LocalExists
        RespondsTo
        SpawnProcess
        SendProcessMessage
        ReceiveProcessMessage
        GetCurrentPid
        SetParentLocal
        GetParentLocal
        FileReadExact
        StdinReadExact
        ObjectEquals
        GetToplevel
        GetNilPrototype
        GetNil
        AttrExists
        RemoveAttribute
        GetAttributes
        GetAttributeNames
        MonotonicTimeNanoseconds
        MonotonicTimeMilliseconds
        GetGlobal
        SetGlobal
        SendMessage
        Throw
        SetRegister
        TailCall
      ]
        .each_with_index
        .each_with_object({}) { |(value, index), hash| hash[value] = index }
        .freeze

      attr_reader :index, :arguments, :location

      def self.named(name, arguments, location)
        new(NAME_MAPPING[name], arguments, location)
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
